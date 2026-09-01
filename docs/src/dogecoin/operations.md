Operations
==========

Health checks
-------------

There is no metrics endpoint. No Prometheus exporter, no `/metrics`, no
counters over HTTP. What exists:

| Signal | Where | Use it for |
| --- | --- | --- |
| `GET /api/v1/capabilities` | HTTP | Liveness, indexed height, chain identity, index capabilities. **This is the health check.** |
| `GET /block-count` | HTTP | Plain-text indexed block count. |
| `GET /status` | HTTP | Unrecoverable-reorg detection, by body text. |
| `ord info` | CLI | Index file size, page statistics, outputs traversed. Writer: cannot run alongside the server. |
| `ord info --transactions` | CLI | Actual indexing rate in blocks per minute. Writer. |
| Log lines | journal | Per-block progress, commits, savepoints, RPC retries. |

### Why `/status` is not a health check

`/status` returns **HTTP 200 in both cases**. Healthy is the body `OK`;
broken is the body `unrecoverable reorg detected, please rebuild the
database.` Any monitor that checks only the status code will report a
permanently broken index as healthy.

### The check that actually works

```shell
curl -fsS --max-time 20 http://127.0.0.1:8391/api/v1/capabilities
```

Then assert, as `deploy/linux/universe-dogecoin-health` does:

- `.chain == "dogecoin"` (you are talking to the right index)
- `.block_count` is a number and is **increasing** between polls
- `.drc20`, `.dunes`, `.transactions` match what this instance is supposed to
  provide
- the node's height minus `(.block_count - 1)` is within your lag budget

What to alert on
----------------

| Alert | Condition | Why |
| --- | --- | --- |
| **Index stalled** | `block_count` unchanged for longer than your stall budget | The server keeps answering with stale data. This is the primary failure mode. `deploy/linux/universe-dogecoin-health` uses 7200 seconds. |
| **Index lagging** | node height minus indexed height above your budget | On Dogecoin, 100 blocks is roughly 100 minutes. The reference health check uses 100 blocks. |
| **Unrecoverable reorg** | `/status` body is not `OK` | Requires a rebuild or a restore. Page someone. |
| **Wrong chain** | `chain` is not `dogecoin`, or the indexed block hash does not match the node's hash at that height | Points at a mixed-up data directory or a node on another network. |
| **Capability mismatch** | a capability you require reports `false` | The database was built without the flag. No restart fixes it. |
| **Disk headroom** | free space below your floor | The reference check uses 100 GiB on the index volume and 200 GiB on a build volume. redb grows and `--compact` needs room alongside the file. |
| **Node unhealthy** | `getblockchaininfo` fails, or `initialblockdownload` is true | The indexer cannot be healthier than its node. |
| **RPC pressure** | repeated `failed to fetch block N, retrying in Ss` | Lower `--nr-parallel-requests` or give the node more capacity. |

Do not alert on process liveness alone. This process stays up while doing
nothing useful.

Backups
-------

**A file copy of an open redb database is not a backup.** redb has no external
consistent-snapshot tool, and the file is being mutated under you. The runbook
in `deploy/DOGECOIN-SNAPSHOT-RESTORE.md` states the rule directly: never copy a
redb file while its owning process has it open, and never stop or restart a
process that is indexing, repairing or committing.

The supported sequence:

1. Stop the service gracefully and wait for it to exit. `SIGINT`, `SIGTERM` or
   `SIGHUP` all reach the handler; a second signal calls `process::exit(1)` and
   skips the clean shutdown, so send exactly one.
2. Confirm no process holds the file open (`lsof` on the database path).
3. Copy or snapshot the file.
4. Record a digest of the copy, and verify it.
5. Start the service again.

`deploy/aws-doge-index/` contains the tooling used to move a large index this
way: chunked copy, parallel per-chunk digests, chunk repair for an interrupted
copy, and a validation pass before cutover. Read
`deploy/aws-doge-index/README.md` before attempting anything similar.

If you cannot take downtime, take the snapshot from a second instance that is
allowed to stop, or from a filesystem or volume snapshot taken after a clean
stop. There is no correct hot copy.

Recovery
--------

| Situation | Action |
| --- | --- |
| Process crashed or was killed | Start it again. Everything since the last 1000-block commit is reindexed automatically. |
| Database will not open, schema mismatch | Restore a snapshot at the right schema version, or rebuild. There is no in-place migration. |
| Unrecoverable reorg | Restore a snapshot from before the fork point and let it catch up, or rebuild. |
| Address-index answers look wrong | `ord repair-address-index`, below. |
| Reopening takes an extremely long time | The database was not closed cleanly. Let it finish; redb is repairing. Do not kill it. |

Rebuilding is the last resort, and on a full mainnet index it is a multi-day
operation. Keep snapshots.

Repairing the address index
---------------------------

Symptoms: `/address/:address`, `/utxos/balance/:address`,
`/inscriptions/balance/:address` or `/api/v1/funding/:address` return outputs
that have already been spent, or miss outputs the node says exist.

```shell
systemctl stop universe-ord-dogecoin-full.service   # or your unit
ord --index=/data/ord-dogecoin/index-all.redb repair-address-index
systemctl start universe-ord-dogecoin-full.service
```

- It is a writer: stop the server first.
- It is resumable. An interrupted run continues from
  `ADDRESS_INDEX_REPAIR_STATE`.
- `--address-batch-size` (default 1000) trades memory against commit
  frequency.
- `--compact` rewrites the whole database file afterwards. On a large index
  that takes a long time and needs free space alongside the existing file. Do
  not pass it casually.

It prints `addresses_scanned`, `live_outputs_recorded`,
`stale_outputs_removed`, `batches_committed`, `compacted`, and the file size
before and after.

Upgrades
--------

There is no versioned upgrade path, because there is no release line. See
[Releases and versioning](releases.md).

To move an instance to a newer commit:

1. Build the new binary and check `SCHEMA_VERSION` in `src/index.rs`. **If it
   changed, the existing database cannot be reused.** Plan a rebuild or a
   snapshot restore before you touch production.
2. Stop the service gracefully.
3. Take a backup.
4. Replace the binary, and replace `subsidies.json` and `starting_sats.json`
   alongside it. They are part of the artifact: an index built with one
   schedule and served with another will report a different
   `subsidy_schedule_hash`.
5. Start, and watch the first few blocks index.

The reference systemd units install each build under a
release directory named for its commit and point `ExecStart`,
`SUBSIDIES_PATH` and `STARTING_SATS_PATH` at that same directory. That keeps
the binary and its two data files pinned together, and makes a rollback a unit
edit rather than a file shuffle.

Migration between hosts
-----------------------

Moving a database file to another host is supported: the file is
self-describing, and the feature flags travel with it. The constraints are
schema version and redb version, both of which must match the binary that will
open it.

`deploy/aws-doge-index/` documents a full worked migration, including why an
`rsync --inplace` delta is the wrong tool for the final consistency pass on a
multi-terabyte file: it read and rewrote every block serially at queue depth
one, measured at 3.1 MB/s, while the volume sat 96 percent idle. Parallel
chunked copy with per-chunk digests is used instead.

Running two indexes
-------------------

A common arrangement is two databases on one host: a cheap inscriptions-only
index, and a full index with DRC-20, Dunes and transactions. They are separate
files, separate processes, separate ports, and separate feature flags. The
reference units in `deploy/linux/` do exactly this.

Each database still allows only one writer. Two processes must never point at
the same file.
