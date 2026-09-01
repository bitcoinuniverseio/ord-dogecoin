Troubleshooting
===============

Organized by what you actually observe.

The process exits immediately with a panic about an environment variable
--------------------------------------------------------------------------

```
STARTING_SATS_PATH must be set
SUBSIDIES_PATH must be set
```

Both variables are mandatory and both are read with `expect()`. Set them to
absolute paths to the `starting_sats.json` and `subsidies.json` files that
shipped with the binary you are running.

In systemd this is two `Environment=` lines. In Docker the image already places
both files at `/starting_sats.json` and `/subsidies.json`, and
`docker-compose.yaml` sets the variables to match. See
[Configuration](configuration.md#required-environment-variables).

DRC-20 or Dunes endpoints return HTTP 400 saying the index was created without a flag
------------------------------------------------------------------------------------

```
this index was created without --index-drc20 and cannot serve DRC-20 state;
the flag is fixed at database creation and requires a rebuild
```

The message is exact. The flag is stored in the database file at creation and
read back on every open. Adding it to your command line does nothing, and there
is no backfill: the historical protocol state was never parsed.

Confirm with `GET /api/v1/capabilities`. If the capability you need is `false`,
your options are a rebuild with the flag, or a snapshot that already has it.

DRC-20 endpoints return an empty list and you expected tokens
-------------------------------------------------------------

Check `drc20_index_enabled` in the payload, or `/api/v1/capabilities`. If it is
`true`, the index genuinely holds no matching tokens at this height. If it is
`false`, see the previous entry.

This distinction is the entire reason `/api/v1/capabilities` exists. The legacy
`/drc20/*` routes do not carry it.

`/api/v1/funding/{address}` returns HTTP 400
---------------------------------------------

Two causes. The first:

```
this index was created without --index-transactions and cannot serve funding
proofs; the flag is fixed at database creation and requires a rebuild
```

The endpoint needs raw previous transactions and cannot fabricate them. Rebuild
with the flag.

The second:

```
funding address must use its canonical encoding
```

The address you passed is not byte-identical to how the parser re-encodes it.
Pass the address exactly as the chain encodes it.

The index never advances, but the server keeps answering
--------------------------------------------------------

The indexer runs inside the server process, so the HTTP surface stays up while
indexing is dead. Work through these in order:

1. `GET /status`. If the body is `unrecoverable reorg detected, please rebuild
   the database.`, see below.
2. Check the log for `would sleep for more than 120s, giving up`. The fetch
   thread abandoned the node.
3. Check the log for `failed to fetch block N, retrying in Ss`. The node is
   under pressure: lower `--nr-parallel-requests`, or raise the node's
   `rpcthreads` and `rpcworkqueue`.
4. Check the node: `getblockchaininfo` must succeed and
   `initialblockdownload` must be `false`.
5. Check free disk on the index volume.

`/status` says `unrecoverable reorg detected`
----------------------------------------------

The indexer could not find a common ancestor within about 40 blocks. It has
stopped advancing, and everything it serves from now on is suspect. `/status`
still returns HTTP 200; only the body changed.

Before assuming a real 40-block Dogecoin reorg, which would be extraordinary,
check whether the node changed identity: reindexed, restored from a different
snapshot, or switched network. A node with different history produces exactly
this error.

Recovery is a restore from a snapshot predating the fork point, or a rebuild.
There is no automated repair. See [Reorgs and mempool](reorgs.md).

The database will not open
--------------------------

```
index at `...` appears to have been built with an older, incompatible version of ord,
consider deleting and rebuilding the index: index schema N, ord schema 6
```

or the same message with "newer" and "consider updating ord".

The schema version in the file does not match `SCHEMA_VERSION` in the binary.
There is no in-place migration. Use a binary at the matching schema version, or
restore a snapshot built at the version you have.

A separate failure mode: a database written by a `wonky-ord` build on an older
redb release is not file-compatible with the redb 2.4.0 this build uses, even
if the ord schema number matches.

Starting the process takes an extremely long time
-------------------------------------------------

The database was not closed cleanly and redb is repairing it. Let it finish.
`TimeoutStartSec=10min` in the reference unit exists for this.

Prevent it: stop gracefully, once, and never `SIGKILL` a writer. In Docker,
`docker compose stop -t 600`. In systemd, `KillSignal=SIGINT`,
`SendSIGKILL=no`, `TimeoutStopSec=30min`.

A second signal during shutdown calls `process::exit(1)` immediately and skips
the clean close. Send one signal and wait.

Another process cannot open the database
----------------------------------------

redb allows one writer. `ord server`, `ord index`, `ord info`, `ord find`,
`ord balances` and `ord dunes` are all writers, because they call
`index.update()`. Only `ord repair-address-index` opens the index without
advancing it, and it is still a writer.

Stop the server before running any of them against the same file.

Address balances include outputs that were already spent
--------------------------------------------------------

Two possibilities.

**The output was spent by an unconfirmed transaction.** There is no mempool
here. That is expected behaviour and not a bug. Apply your own reservations.

**The address index has stale rows.** Stop the service and run
`ord repair-address-index`. See
[Operations](operations.md#repairing-the-address-index).

Ordinal, rarity or `/sat` routes return errors
----------------------------------------------

```
find requires index created with `--index-sats` flag
```

`--index-sats` was not set when the database was created, and cannot be added.
`/sat/:sat`, `/range/:start/:end`, `/rare.txt`, `ord find` and `ord list` all
depend on it.

`ord wallet` fails with unexpected output descriptors
-----------------------------------------------------

```
wallet "ord" contains unexpected output descriptors, and does not appear to be
an `ord` wallet, create a new wallet with `ord wallet create`
```

This cannot be fixed. The check requires two Bitcoin Taproot descriptors, which
Dogecoin Core cannot produce. The wallet subcommands are inherited dead code.
See [Differences from Bitcoin ord](differences.md#the-wallet-subcommands-do-not-work-on-dogecoin).

The indexer refuses to start against the node
---------------------------------------------

```
Dogecoin RPC server is on testnet but ord is on mainnet
```

`--chain` and the node disagree. Note that the data directory layout also
depends on the chain, so a mismatch usually means you are also pointed at the
wrong database.

```
failed to connect to Dogecoin Core RPC at ... using cookie file ...
```

Authentication precedence: if the cookie file exists it is used; otherwise
credentials are parsed from `--rpc-url`. A cookie file that exists but is stale
or unreadable will fail before the URL credentials are ever tried.

Indexing is much slower than expected
-------------------------------------

In order of likelihood:

1. The node is not local, or is serving other traffic.
2. Storage is not NVMe, or has a low effective queue depth.
3. `--nr-parallel-requests` is set too high and the retry backoff is
   dominating. Check the log for retries.
4. `--index-sats` is enabled and you did not need it.
5. `--first-inscription-height` is much lower than it needs to be, so full
   blocks are being downloaded where headers would do.

Measure with `ord info --transactions`, which prints blocks indexed per
write-transaction window and the elapsed minutes. Stop the server first.

An inscription is missing or its content is wrong
-------------------------------------------------

- **The inscription is below `--first-inscription-height`.** Blocks below that
  height were fetched as headers only. Lowering the option does not rescan; the
  index must be rebuilt.
- **The inscription chain is incomplete.** A multi-transaction inscription
  stays in `PARTIAL_TXID_TO_INSCRIPTION_TXIDS` until the final transaction
  confirms. It is not an inscription yet.
- **The inscription delegates its content.** `/content` serves the delegate's
  bytes, and `/api/v1/inscriptions` reports the delegate's content type and
  length. That is intended.
- **The inscription is hidden.** An id listed under `hidden` in the YAML config
  is not served by this instance's content and preview routes. It is still in
  the index.

Two instances disagree about ordinals
-------------------------------------

Compare `subsidy_schedule_hash` from `/api/v1/inscriptions` on both. It is the
SHA-256 of the `subsidies.json` bytes each instance loaded. Different hashes
mean different subsidy schedules, which means different ordinal numbering. Ship
the subsidy files with the binary.
