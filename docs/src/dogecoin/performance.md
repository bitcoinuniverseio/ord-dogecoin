Performance and sizing
======================

Choose the index shape first
----------------------------

Cost is dominated by which feature flags you enable, and they cannot be changed
later.

| Configuration | What it can answer | Relative cost |
| --- | --- | --- |
| Inscriptions only (no flags) | Doginals, ownership, content | Baseline |
| `+ --index-transactions` | Raw transactions, funding proofs, faster API | Moderate increase |
| `+ --index-drc20` | DRC-20 catalog, balances, holders. Requires transactions. | Moderate increase |
| `+ --index-dunes` | Dunes catalog and balances | Moderate increase |
| `+ --index-sats` | Ordinal ranges, rarity, `ord find`, `/sat`, `/range` | **Large increase.** Enable only if you need ordinal-level queries. |

Disk
----

The figures the repository actually records:

| Configuration | Size | Source |
| --- | --- | --- |
| Inscriptions only, pre-indexed | roughly 300 GB | `README.md`, referring to the upstream download |
| Full index (transactions, DRC-20, Dunes) | "around 400gb" | `README.md` |
| A production full index that was migrated | roughly 1.5 TB | `deploy/aws-doge-index/README.md` |

The 400 GB figure is upstream's and is now low. **Size for the 1.5 TB order of
magnitude** for a full index that will keep running, and add headroom on top:

- redb accumulates fragmentation. `ord info` reports `fragmented_bytes`.
- `--compact` needs free space alongside the existing file.
- A backup on the same volume doubles the requirement.
- Inscription content has **no size ceiling** on this chain, so the corpus
  grows in ways a Bitcoin index does not. See
  [Differences from Bitcoin ord](differences.md#there-is-no-content-size-ceiling).

The reference health check alerts below 100 GiB free on the index volume and
200 GiB on a build volume.

Storage class matters more than storage size. redb writes randomly across a
very large file. Local NVMe is the target. Network block storage works if it
sustains real parallel IOPS, and fails badly at low queue depth.

Memory
------

`--db-cache-size` sets the redb page cache in bytes. Default is one quarter of
total system memory.

| Guidance | Value |
| --- | --- |
| Reference full-index unit | 8 GiB cache, `MemoryHigh=10G`, `MemoryMax=12G` |
| Auto-tuning used by the migration bootstrap | half of RAM, capped at 64 GiB |

The cache is a floor for resident memory, not a ceiling on the process. Always
set the cgroup or systemd memory limit above the cache size, with room for the
value cache, the block channel and the HTTP server. The reference unit leaves 2
to 4 GiB above the cache.

More cache mainly helps the initial index, where the working set is enormous.
In steady state, a few gigabytes is plenty.

CPU
----

Indexing is not CPU bound in normal operation. It is bound by the node's RPC
throughput and by storage. 8 cores is comfortable for a full index plus the
HTTP server; the block fetch thread, the value fetcher and the HTTP runtime
each want their own.

RPC concurrency
---------------

`--nr-parallel-requests` (default 12) is the number of `getrawtransaction`
calls batched into one JSON-RPC POST.

| Situation | Value | Source |
| --- | --- | --- |
| Dedicated node, default settings | 16 | `README.md` |
| Shared node reached across a tunnel | 4 | `deploy/aws-doge-index/`, where higher values made transaction lookups return HTTP 500 and replayed the same uncommitted batch |
| Node specifically tuned for it | up to 250 | `docker-compose.yaml` |
| Auto-tuned by the migration bootstrap | `nproc * 2`, capped at 4 | `deploy/aws-doge-index/README.md` |

The spread is not a contradiction. The right value is whatever the node can
serve without erroring, and nothing more. Raising it past that point makes
throughput **worse**, because the retry backoff doubles on every failure.

Watch for `failed to fetch block N, retrying in Ss` in the log. If you see it
regularly, the value is too high, or the node needs more `rpcthreads` and
`rpcworkqueue`.

Serving
-------

The HTTP server is `axum` on a Tokio multi-thread runtime, reading through redb
read transactions. Reads do not block the writer.

The expensive routes are the ones that scan:

- `/api/v1/drc20/transferables` is not paginated and walks every outstanding
  transferable, looking up token info, inscription entry and satpoint for each
  one.
- Unpaginated address routes (`/utxos/balance/:address` without a page,
  `/dunes/balance/:address` without a page) are unbounded by address size.
- `/content/:id` for a large inscription is a large response. There is no size
  cap on inscription content.

Prefer the bounded `/api/v1` routes for anything automated, and put a cache in
front of `/content`.

A realistic starting point
--------------------------

For a full mainnet index serving an internal API:

| Resource | Value |
| --- | --- |
| CPU | 8 cores |
| RAM | 16 GiB, with `--db-cache-size=8589934592` |
| Disk | 2 TB NVMe, alerting below 100 GiB free |
| Node | Dogecoin Core 1.14.9, `txindex=1`, same host or same rack |
| `--nr-parallel-requests` | start at 16, lower if the log shows retries |

For an inscriptions-only index, halve the RAM and start at 500 GB of disk.
