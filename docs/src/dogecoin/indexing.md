Indexing
========

How a block gets indexed
------------------------

`Updater::update_index` runs one loop. A dedicated thread fetches blocks from
Dogecoin Core ahead of the loop through a bounded channel of 32 blocks, so RPC
latency and index work overlap.

For each block:

1. **Reorg check.** `Reorg::detect_reorg` compares the block's
   `prev_blockhash` against the hash the index recorded for the previous
   height. See [Reorgs and mempool](reorgs.md).
2. **Input values.** Missing outpoint values are requested from the node with
   batched `getrawtransaction` calls, `--nr-parallel-requests` at a time, and
   cached.
3. **Inscriptions.** `InscriptionUpdater` walks the transactions, parses
   envelopes out of `scriptSig`, assigns inscription numbers and ordinals,
   moves existing inscriptions to their new satpoints, and tracks partial
   multi-transaction inscriptions.
4. **Dunes,** if `--index-dunes`: `DuneUpdater` deciphers dunestones, applies
   etchings, mints and edicts, and updates per-outpoint dune balances.
5. **DRC-20,** if `--index-drc20`: `Drc20Updater` replays the inscription
   operations collected in step 3 as DRC-20 deploys, mints, inscribe-transfers
   and transfers, and updates balances and the transferable log.
6. **Savepoint,** every 10 blocks when within 25 blocks of the tip.

The write transaction commits **every 1000 blocks**, and again at shutdown for
whatever is uncommitted. A crash therefore costs at most the blocks since the
last commit, which are simply reindexed on the next start.

Blocks below `--first-inscription-height` are fetched as **headers only**,
unless `--index-sats` is set. This is why the height matters so much: it is the
difference between downloading five million block headers and downloading five
million full blocks.

Initial indexing
----------------

Initial indexing of Dogecoin mainnet from genesis is a multi-day job. The
repository does not record a single authoritative wall-clock figure, and the
honest answer is that it depends almost entirely on the Dogecoin Core node's
ability to serve `getrawtransaction`, not on the indexer.

What the repository does record:

| Fact | Source |
| --- | --- |
| A full index from scratch "can take many days" | `README.md` |
| A pre-indexed inscriptions-only database is roughly 300 GB | `README.md`, referring to the upstream `legacy.trac.network` download |
| A full index with DRC-20, Dunes and transactions needs "around 400gb" | `README.md` |
| A production full index of about 1.5 TB was migrated in `deploy/aws-doge-index/` | `deploy/aws-doge-index/README.md` |

The 400 GB figure is upstream's and is now low. Treat the 1.5 TB figure from
the migration tooling as the realistic order of magnitude for a full index that
has been running, and see [Performance and sizing](performance.md).

### Making the initial index finish

The three things that actually matter, in order:

1. **The node must be local and fast.** Every transaction lookup is a
   round trip. A node behind a tunnel or on another host will dominate the
   runtime. `deploy/aws-doge-index/README.md` records that a shared Dogecoin
   Core endpoint reached across a reverse tunnel could not sustain more than
   four concurrent requests before transaction lookups started returning HTTP
   500 and the same uncommitted batch was replayed.
2. **Storage must be NVMe.** redb writes randomly across a very large file.
   Network block storage with a low queue depth is the second most common
   reason an index never finishes.
3. **`--nr-parallel-requests` must fit the node.** Raising it past what the
   node can serve makes throughput worse, not better, because of the retry
   backoff. Start at 16 against a dedicated node, and lower it if you see
   `failed to fetch block ..., retrying in Ns` in the log.

### Chunked and resumable catch-up

`--height-limit` stops indexing at a fixed height, which makes an initial index
resumable in defined chunks and makes a partially built database verifiable
against an expected height. The tooling in `deploy/aws-doge-index/` uses this,
along with parallel chunk digests, to migrate and verify a large index without
a single long-running unverifiable copy.

### Starting from a snapshot

The fastest initial index is one you do not run. Restoring a database file
built with the same schema version and the same feature flags is supported by
design: point `--index` at the restored file.

Two constraints:

- **The schema version must match.** This build is schema 6. An older or newer
  database is refused at open with an explicit message. See
  [Database model](database.md#schema-version).
- **The feature flags come from the file, not from your command line.** A
  snapshot built without `--index-drc20` gives you a database that cannot
  answer DRC-20 questions, whatever flags you pass. Check with
  `/api/v1/capabilities` before you trust it.

The upstream pre-indexed download referenced in `README.md` is an
inscriptions-only database: no DRC-20, no Dunes. It is a third-party artifact,
not published by this organization, and nothing in this repository verifies it.

Incremental indexing
--------------------

Once caught up, `ord server` keeps the index current from its own background
thread:

```
loop {
  if shutting down { break }
  index.update()          // indexes every block available
  sleep(5 seconds)
}
```

At Dogecoin's one minute block target, a five second poll means the index is
normally within a few seconds of a new block, and the work per pass is one
block. Steady-state cost is negligible compared to the initial index.

Because the indexer lives inside the server process, **an index that stops
advancing does not stop the server**. It keeps serving stale answers with a
`block_count` that no longer moves. That is the failure mode to alert on. See
[Operations](operations.md#what-to-alert-on).

Do not run `ord index` against the same database file while `ord server` is
running. redb permits a single writer; the second process will fail to open the
database for writing. `ord info` also calls `index.update()` before printing,
so it is a writer too.

Reading progress
----------------

At `RUST_LOG=info` the progress bar is disabled and progress is logged. The
useful signals:

| Log line | Meaning |
| --- | --- |
| `Block N at <time> with M transactions...` | One block entering the indexer. This is your progress signal. |
| `Committing at block height N, ...` | A 1000-block batch committed. |
| `creating savepoint at height N` | Near the tip, savepoint written. |
| `failed to fetch block N, retrying in Ss` | Node pressure. The backoff doubles each attempt. |
| `would sleep for more than 120s, giving up` | The fetch thread has given up on the node. Indexing stops. |
| `N block deep reorg detected at height H` | Recoverable reorg, rollback follows. |
| `unrecoverable reorg detected` | The database must be rebuilt. |

The machine-readable equivalent is `GET /api/v1/capabilities`, which returns
`block_count` and `block_hash`. `block_count` is the number of indexed blocks,
so the indexed tip height is `block_count - 1`.
