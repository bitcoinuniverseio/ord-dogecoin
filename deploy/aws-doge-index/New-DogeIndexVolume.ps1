[CmdletBinding(SupportsShouldProcess)]
param(
    [string] $Region = 'eu-central-1',
    [string] $AvailabilityZone = 'eu-central-1a',
    [ValidateRange(2500, 16384)] [int] $SizeGiB = 3200,
    [ValidateRange(3000, 80000)] [int] $Iops = 6000,
    [ValidateRange(125, 2000)] [int] $ThroughputMiB = 250,
    [switch] $AllowCreate
)

$ErrorActionPreference = 'Stop'
$filters = @(
    'Name=tag:Project,Values=doge-index-catchup',
    'Name=tag:Persistence,Values=retain',
    'Name=status,Values=creating,available,in-use'
)
$existingJson = aws ec2 describe-volumes --region $Region --filters $filters --output json
if ($LASTEXITCODE -ne 0) { throw 'Unable to inspect existing Doge index volumes' }
$existing = @((ConvertFrom-Json ($existingJson -join "`n")).Volumes)

if ($existing.Count -gt 1) {
    throw "Found $($existing.Count) retained Doge index volumes. Refusing to choose or create another."
}
if ($existing.Count -eq 1) {
    $volume = $existing[0]
    if ($volume.AvailabilityZone -ne $AvailabilityZone) {
        throw "Existing volume $($volume.VolumeId) is in $($volume.AvailabilityZone), not $AvailabilityZone"
    }
    $volume | Select-Object VolumeId, AvailabilityZone, State, Size, Iops, Throughput, Encrypted | Format-List
    return
}

if (-not $AllowCreate) {
    throw 'No retained Doge index volume exists. Re-run with -AllowCreate after verifying region, zone, size, IOPS, and throughput.'
}

$token = 'universe-doge-index-persistent-eu-central-1a-v1'
$tags = 'ResourceType=volume,Tags=[{Key=Name,Value=doge-index-persistent},{Key=Project,Value=doge-index-catchup},{Key=Persistence,Value=retain},{Key=DataClass,Value=critical-index},{Key=DeleteProtection,Value=required},{Key=SourceHost,Value=universe-indexers},{Key=Capacity,Value=spot-only}]'
if ($PSCmdlet.ShouldProcess("$AvailabilityZone $SizeGiB GiB gp3", 'Create retained Dogecoin index volume')) {
    aws ec2 create-volume --region $Region --availability-zone $AvailabilityZone --volume-type gp3 --size $SizeGiB --iops $Iops --throughput $ThroughputMiB --encrypted --client-token $token --tag-specifications $tags --output json
    if ($LASTEXITCODE -ne 0) { throw 'Persistent volume creation failed' }
}
