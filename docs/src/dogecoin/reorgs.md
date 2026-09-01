Reorgs and mempool
==================

Mempool: there isn't one
------------------------

There is no mempool code in this repository. No call to `getrawmempool`, no
unconfirmed transaction tracking, no zero-confirmation view. Every table, every
HTML page and every API response is derived from **confirmed blocks only**.

Consequences worth stating plainly:

- An inscription in the mempool does not exist here until it confirms.
- A DRC-20 transfer that has been broadcast but not mined does not move a
  balance here.
- An output spent by an unconfirmed transaction still appears as unspent in
  `/api/v1/funding/:address` and in the address index. The endpoint's own
  documentation says so: callers "must apply any additional protocol
  reservations owned by downstream indexers before treating an output as
  spendable". Double-spend protection is the caller's problem, not this
  indexer's.
- Fee estimation is not available. There is no code here that looks at
  mempool state.

If you need an unconfirmed view, get it from Dogecoin Core directly.

How reorgs are detected
-----------------------

Before indexing block at height `h`, `Reorg::detect_reorg` compares the block's
`prev_blockhash` with the hash the index stored for height `h - 1`.

If they match, indexing proceeds. If they differ, the code walks backwards
comparing the index's block hash at each depth against the node's block hash at
the same height, looking for the fork point.

The search bound is:

```
max_recoverable_reorg_depth = (MAX_SAVEPOINTS - 1) * SAVEPOINT_INTERVAL
                              + height % SAVEPOINT_INTERVAL
```

With `MAX_SAVEPOINTS = 5` and `SAVEPOINT_INTERVAL = 10`, that is between 40 and
49 depending on the height, and the search runs to one less than the bound. In
round terms: **a reorg up to about 40 blocks deep is recoverable, anything
deeper is not.**

On Dogecoin, 40 blocks is roughly 40 minutes of chain. That is a much shorter
wall-clock window than the same block count on Bitcoin. See
[Differences from Bitcoin ord](differences.md#a-confirmation-here-is-worth-roughly-a-tenth-of-a-bitcoin-confirmation).

Savepoints
----------

`Reorg::update_savepoints` writes a redb persistent savepoint when **both**
conditions hold:

- the height is below 10, or divisible by 10; and
- the indexed height is within 25 blocks of the node's block count.

The second condition is why savepoints are not written during initial indexing.
Historical catch-up produces no rollback points at all, which is correct: a
reorg cannot affect a height that is five million blocks behind the tip.

At most five savepoints are retained. When a sixth is due, the oldest is
deleted first.

Recovery
--------

On a recoverable reorg, `Reorg::handle_reorg` restores the **oldest** retained
persistent savepoint and commits, then logs:

```
rolling back database after reorg of depth D at height H
successfully rolled back database to height N
```

Note that this is deliberately blunt. It does not restore the savepoint nearest
the fork point; it restores the oldest one it still holds, which can be up to
about 40 blocks before the reorg. The blocks between there and the new tip are
then reindexed from the node on the next pass. The cost is a minute or two of
reindexing at Dogecoin block rates. The benefit is that recovery does not
depend on picking the right savepoint.

The updater then restarts and indexes forward along the new chain. No operator
action is required.

Unrecoverable reorgs
--------------------

If no common ancestor is found within the search bound, the index sets an
internal flag and returns `ReorgError::Unrecoverable`. From then on:

- `GET /status` returns **HTTP 200** with the body
  `unrecoverable reorg detected, please rebuild the database.`
- The index stops advancing.
- The server keeps serving, from data that is now known to be wrong.

**`/status` returns 200 in both the healthy and the unrecoverable case.** An
HTTP status code check is not a health check for this service. Alert on the
response body, or on `block_count` from `/api/v1/capabilities` failing to
advance. See [Operations](operations.md#what-to-alert-on).

There is no automated repair. The database must be rebuilt, or restored from a
snapshot taken before the fork point and allowed to catch up.

A reorg deeper than 40 blocks on Dogecoin mainnet would be an extraordinary
event. If you see this in normal operation, suspect the node before you suspect
the chain: a node that was reindexed, restored from a different snapshot, or
switched between mainnet and another network will present a completely
different history and produce exactly this error.

What the Universe registry records
----------------------------------

For `doginals` and `drc20`, the published capability snapshot records
`automaticReconciliation: true` with the policy: "The authority appends reorg
observations, reopens or invalidates affected orders deterministically, and
preserves funding, broadcast, and replacement lineage."

That describes the downstream authority that consumes this index, not this
indexer. `ord-dogecoin` itself has no notion of orders, and its own reorg
handling is the savepoint rollback described above.
