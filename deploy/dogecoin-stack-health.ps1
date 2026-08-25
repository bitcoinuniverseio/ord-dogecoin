<#
.SYNOPSIS
  Health check for the Universe Dogecoin chain, index and authority stack.

.DESCRIPTION
  On 2026-08-23 the whole Dogecoin stack went down and stayed down until a
  user reported empty index screens. Nothing alerted, because:

    - DogecoinCoreTxIndex declared a hard service dependency on DogecoinCore,
      whose WinSW configuration and data directory had been removed, so
      Windows silently refused to start it after the reboot;
    - ord-dogecoin kept answering /block-count from its own stale index, so
      every liveness probe against it still looked healthy;
    - nothing compared the ord index to the node, by height or by identity.

  Separately, the ord index had been created without --index-drc20, so every
  DRC-20 query returned "200 []" -- indistinguishable from a chain with no
  tokens -- and no check could tell the difference.

  Liveness is therefore not enough. This distinguishes:

    process alive / service listening / chain advancing / index advancing /
    chain-index agreement / index capability / authority readiness

  Exit code 0 means healthy, 1 means degraded. Every run appends one JSON line
  to the state file so a trend is inspectable without a database, and a
  degraded run writes an Application event.

.NOTES
  Read-only. It never restarts, reindexes, or stops anything: an indexer that
  is mid-reorg or mid-reindex must be left alone.
#>

[CmdletBinding()]
param(
  [string] $RpcUrl = 'http://127.0.0.1:22566/',
  [string] $CookiePath = 'D:\Services\OrdDogecoin\config\dogecoin-txindex-rpc.cookie',
  [string] $OrdUrl = 'http://127.0.0.1:8390',
  # Empty until the Dogecoin Marketplace authority is deployed; set it then.
  [string] $AuthorityUrl = '',
  [string] $StatePath = 'D:\Services\DogecoinCoreTxIndex\health\dogecoin-stack-health.jsonl',
  # ord may legitimately trail the node while catching up. A frozen index, or
  # an index ahead of the node, is the failure being detected.
  [int] $MaxOrdLagBlocks = 100,
  # Set true once the production index is expected to answer DRC-20.
  [switch] $RequireDrc20,
  # Free space on the volume holding the chain data. The original outage began
  # with "Error reading from database": a chain database that runs out of room
  # mid-write is corrupted, not merely stopped, so headroom is a health signal
  # rather than a capacity report.
  [int] $MinFreeGigabytes = 100,
  [string] $DataPath = 'D:\Services\DogecoinCoreTxIndex\data',
  [int] $TimeoutSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function New-Problem {
  param([string] $Code, [string] $Detail)
  [pscustomobject]@{ code = $Code; detail = $Detail }
}

function Invoke-NodeRpc {
  param([string] $Method, [object[]] $Params = @())
  $cookie = (Get-Content -Path $script:CookiePath -Raw -ErrorAction Stop).Trim()
  $headers = @{
    Authorization  = 'Basic ' + [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes($cookie))
    'content-type' = 'text/plain'
  }
  $body = @{ jsonrpc = '1.0'; id = 'health'; method = $Method; params = $Params } | ConvertTo-Json -Compress -Depth 4
  Invoke-RestMethod -Uri $script:RpcUrl -Method Post -Headers $headers -Body $body -TimeoutSec $script:TimeoutSeconds
}

$script:RpcUrl = $RpcUrl
$script:CookiePath = $CookiePath
$script:TimeoutSeconds = $TimeoutSeconds

$problems = [System.Collections.Generic.List[object]]::new()
$nodeHeight = $null; $nodeHeaders = $null; $nodeBestHash = $null
$nodeIbd = $null; $nodeTipAgeSeconds = $null; $nodeTxIndex = $null
$ordHeight = $null; $ordHash = $null; $ordDrc20 = $null
$freeGigabytes = $null; $chainDataGigabytes = $null
$chainIdentityAgrees = $null
$authorityReadReady = $null; $authorityReason = $null

# --- service lifecycle -----------------------------------------------------
$service = Get-Service -Name DogecoinCoreTxIndex -ErrorAction SilentlyContinue
if ($null -eq $service) {
  $problems.Add((New-Problem 'txindex_service_missing' 'DogecoinCoreTxIndex is not registered.'))
} elseif ($service.Status -ne 'Running') {
  $problems.Add((New-Problem 'txindex_service_not_running' "DogecoinCoreTxIndex is $($service.Status)."))
}
$ordService = Get-Service -Name OrdDogecoinAuthority -ErrorAction SilentlyContinue
if ($null -eq $ordService) {
  $problems.Add((New-Problem 'ord_service_missing' 'OrdDogecoinAuthority is not registered.'))
} elseif ($ordService.Status -ne 'Running') {
  $problems.Add((New-Problem 'ord_service_not_running' "OrdDogecoinAuthority is $($ordService.Status)."))
}

# A dependency on a service that no longer exists is what caused the outage.
$missingDependencies = @()
foreach ($name in 'DogecoinCoreTxIndex', 'OrdDogecoinAuthority') {
  $key = Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\$name" -ErrorAction SilentlyContinue
  $declared = if ($key -and $key.PSObject.Properties.Name -contains 'DependOnService') { $key.DependOnService } else { @() }
  foreach ($dependency in @($declared)) {
    if ([string]::IsNullOrWhiteSpace($dependency)) { continue }
    if (-not (Get-Service -Name $dependency -ErrorAction SilentlyContinue)) {
      $missingDependencies += "$name -> $dependency"
    }
  }
}
if ($missingDependencies.Count -gt 0) {
  $problems.Add((New-Problem 'service_dependency_missing' ($missingDependencies -join '; ')))
}

# --- storage headroom ------------------------------------------------------
try {
  $qualifier = (Split-Path -Qualifier $DataPath).TrimEnd(':')
  $drive = Get-PSDrive -Name $qualifier -ErrorAction Stop
  $freeGigabytes = [math]::Round($drive.Free / 1GB, 1)
  if ($freeGigabytes -lt $MinFreeGigabytes) {
    $problems.Add((New-Problem 'chain_storage_low' "$($qualifier): has $freeGigabytes GB free, below the $MinFreeGigabytes GB floor; a chain database that runs out of room mid-write is corrupted, not merely stopped."))
  }
} catch {
  $problems.Add((New-Problem 'chain_storage_unknown' $_.Exception.Message))
}

# --- Dogecoin Core ---------------------------------------------------------
try {
  $chain = Invoke-NodeRpc -Method 'getblockchaininfo'
  if ($null -ne $chain.error -and $null -ne $chain.error.message) {
    # A warming node reports -28 with a phase message; that is a state, not a fault.
    $problems.Add((New-Problem 'node_warming' $chain.error.message))
  } else {
    $nodeHeight = [int] $chain.result.blocks
    $nodeHeaders = [int] $chain.result.headers
    $nodeBestHash = [string] $chain.result.bestblockhash
    $nodeIbd = [bool] $chain.result.initialblockdownload
    if ($chain.result.PSObject.Properties.Name -contains 'mediantime') {
      $tipTime = [DateTimeOffset]::FromUnixTimeSeconds([long] $chain.result.mediantime)
      $nodeTipAgeSeconds = [int] ([DateTimeOffset]::UtcNow - $tipTime).TotalSeconds
    }
  }
} catch {
  $problems.Add((New-Problem 'node_rpc_unreachable' $_.Exception.Message))
}

# txindex has to be usable, not merely configured.
if ($null -ne $nodeHeight) {
  try {
    $info = Invoke-NodeRpc -Method 'getindexinfo'
    $nodeTxIndex = ($null -eq $info.error)
  } catch {
    # Dogecoin 1.14 predates getindexinfo; fall back to proving a lookup works.
    try {
      $probe = Invoke-NodeRpc -Method 'getrawtransaction' -Params @('0000000000000000000000000000000000000000000000000000000000000000', $true)
      # A missing txid is the expected answer; an "index not enabled" error is not.
      $message = if ($null -ne $probe.error) { [string] $probe.error.message } else { '' }
      $nodeTxIndex = -not ($message -match 'txindex')
    } catch {
      $nodeTxIndex = $null
    }
  }
  if ($nodeTxIndex -eq $false) {
    $problems.Add((New-Problem 'node_txindex_unavailable' 'Dogecoin Core cannot answer transaction lookups.'))
  }
}

# --- ord-dogecoin ----------------------------------------------------------
try {
  $ordCount = Invoke-RestMethod -Uri "$OrdUrl/block-count" -TimeoutSec $TimeoutSeconds
  $ordHeight = [int] $ordCount - 1
} catch {
  $problems.Add((New-Problem 'ord_unreachable' $_.Exception.Message))
}

try {
  $capabilities = Invoke-RestMethod -Uri "$OrdUrl/api/v1/capabilities" -TimeoutSec $TimeoutSeconds
  $ordDrc20 = [bool] $capabilities.drc20
  $ordHash = [string] $capabilities.block_hash
  if ($capabilities.chain -ne 'dogecoin') {
    $problems.Add((New-Problem 'ord_wrong_chain' "ord reports chain '$($capabilities.chain)', expected 'dogecoin'."))
  }
  if ($RequireDrc20 -and -not $ordDrc20) {
    $problems.Add((New-Problem 'ord_drc20_index_absent' 'ord index was created without --index-drc20; DRC-20 queries cannot be answered and the flag cannot be added without a rebuild.'))
  }
} catch {
  # An older ord build predates the capability route; that is itself worth
  # reporting when DRC-20 is expected, because capability cannot be proven.
  if ($RequireDrc20) {
    $problems.Add((New-Problem 'ord_capability_unknown' "ord did not answer /api/v1/capabilities: $($_.Exception.Message)"))
  }
}

# --- chain and index agreement --------------------------------------------
if ($null -ne $nodeHeight -and $null -ne $ordHeight) {
  $lag = $nodeHeight - $ordHeight
  if ([Math]::Abs($lag) -gt $MaxOrdLagBlocks) {
    $problems.Add((New-Problem 'ord_node_height_disagreement' "ord=$ordHeight node=$nodeHeight lag=$lag"))
  }
  # Identity, not just height. A stale or forked ord index can sit at a
  # plausible height while indexing a different chain history entirely.
  if ($null -ne $ordHash -and $ordHeight -le $nodeHeight -and $ordHeight -ge 0) {
    try {
      $atHeight = Invoke-NodeRpc -Method 'getblockhash' -Params @($ordHeight)
      if ($null -eq $atHeight.error) {
        $chainIdentityAgrees = ([string] $atHeight.result -eq $ordHash)
        if (-not $chainIdentityAgrees) {
          $problems.Add((New-Problem 'ord_node_chain_disagreement' "ord indexed hash $ordHash at height $ordHeight, node has $($atHeight.result)"))
        }
      }
    } catch {
      $problems.Add((New-Problem 'chain_identity_unverified' $_.Exception.Message))
    }
  }
}

# --- authority (once deployed) --------------------------------------------
if (-not [string]::IsNullOrWhiteSpace($AuthorityUrl)) {
  try {
    $readiness = Invoke-RestMethod -Uri "$AuthorityUrl/marketplace/v1/doginals/readiness" -TimeoutSec $TimeoutSeconds -SkipCertificateCheck
    $authorityReadReady = [bool] $readiness.readReady
    $authorityReason = [string] $readiness.readReason
    if (-not $authorityReadReady) {
      $problems.Add((New-Problem 'authority_read_not_ready' "readReason=$authorityReason"))
    }
  } catch {
    $problems.Add((New-Problem 'authority_unreachable' $_.Exception.Message))
  }
}

# --- forward progress against the previous run ----------------------------
$previous = $null
if (Test-Path $StatePath) {
  $lastLine = Get-Content -Path $StatePath -Tail 1 -ErrorAction SilentlyContinue
  if ($lastLine) { try { $previous = $lastLine | ConvertFrom-Json } catch { $previous = $null } }
}
if ($null -ne $previous) {
  $behindTip = ($null -ne $nodeHeaders) -and ($null -ne $nodeHeight) -and (($nodeHeaders - $nodeHeight) -gt $MaxOrdLagBlocks)
  if ($behindTip -and $null -ne $previous.nodeHeight -and [int]$previous.nodeHeight -ge $nodeHeight) {
    $problems.Add((New-Problem 'node_not_advancing' "height stuck at $nodeHeight since $($previous.checkedAt)"))
  }
  # ord only has to advance when it is behind the node and the node is ready
  # to feed it. While the node is still catching up, ord waiting is correct.
  $ordShouldAdvance = ($null -ne $nodeHeight) -and ($null -ne $ordHeight) -and ($ordHeight -lt ($nodeHeight - $MaxOrdLagBlocks))
  if ($ordShouldAdvance -and $null -ne $previous.ordHeight -and [int]$previous.ordHeight -ge $ordHeight) {
    $problems.Add((New-Problem 'ord_not_advancing' "ord stuck at $ordHeight while node is at $nodeHeight since $($previous.checkedAt)"))
  }
}

$record = [ordered]@{
  checkedAt            = (Get-Date).ToUniversalTime().ToString('o')
  healthy              = ($problems.Count -eq 0)
  nodeHeight           = $nodeHeight
  nodeHeaders          = $nodeHeaders
  nodeBestHash         = $nodeBestHash
  nodeInitialDownload  = $nodeIbd
  nodeTipAgeSeconds    = $nodeTipAgeSeconds
  nodeTxIndex          = $nodeTxIndex
  freeGigabytes        = $freeGigabytes
  ordHeight            = $ordHeight
  ordIndexedHash       = $ordHash
  ordDrc20Capable      = $ordDrc20
  chainIdentityAgrees  = $chainIdentityAgrees
  authorityReadReady   = $authorityReadReady
  authorityReadReason  = $authorityReason
  problems             = @($problems | ForEach-Object { $_.code })
  details              = @($problems | ForEach-Object { "$($_.code): $($_.detail)" })
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $StatePath) | Out-Null
Add-Content -Path $StatePath -Value ($record | ConvertTo-Json -Compress -Depth 4)

if ($problems.Count -gt 0) {
  Write-Warning ("Dogecoin stack degraded: " + ($record.details -join ' | '))
  try {
    if (-not [System.Diagnostics.EventLog]::SourceExists('UniverseDogecoinHealth')) {
      New-EventLog -LogName Application -Source 'UniverseDogecoinHealth'
    }
    Write-EventLog -LogName Application -Source 'UniverseDogecoinHealth' -EntryType Warning `
      -EventId 4001 -Message ($record | ConvertTo-Json -Depth 4)
  } catch {
    Write-Warning "Could not write the Application event log: $($_.Exception.Message)"
  }
  exit 1
}

Write-Output ("Dogecoin stack healthy: node=$nodeHeight/$nodeHeaders ord=$ordHeight drc20=$ordDrc20 identity=$chainIdentityAgrees freeGB=$freeGigabytes")
exit 0
