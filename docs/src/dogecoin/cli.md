CLI reference
=============

```
ord [GLOBAL OPTIONS] <SUBCOMMAND> [SUBCOMMAND OPTIONS]
```

Global options are documented in [Configuration](configuration.md#global-options).
`ord --help` and `ord <subcommand> --help` print the authoritative list for the
binary you have.

Every subcommand except `parse`, `epochs`, `subsidy` and `traits` opens the
index, and most of them **advance it first**. redb allows one writer, so none of
these can be run against a database that `ord server` has open.

| Subcommand | Opens the index | Advances it first |
| --- | --- | --- |
| `index` | yes | yes (that is the point) |
| `info` | yes | **yes** |
| `server` | yes | yes, from a background thread |
| `repair-address-index` | yes | no |
| `find`, `balances`, `dunes` | yes | **yes** |
| `list`, `preview` | yes | no |
| `parse`, `epochs`, `subsidy`, `traits` | no | no |
| `wallet ...` | non-functional on Dogecoin | |

`ord index`
-----------

Opens the index and runs the updater until it reaches the node's tip, then
exits. This is the initial-sync command.

```shell
ord --rpc-url=http://127.0.0.1:22555 --data-dir=/data/ord-dogecoin \
    --first-inscription-height=4609723 --nr-parallel-requests=16 \
    --index-transactions --index-drc20 --index-dunes \
    index
```

The index feature flags on this first invocation are the ones the database
keeps forever.

`ord server`
------------

Runs the HTTP server **and** the indexer, in one process. Options are in
[Configuration](configuration.md#ord-server-options).

```shell
ord --rpc-url=http://127.0.0.1:22555 --index=/data/ord-dogecoin/index-all.redb \
    server --http --address=127.0.0.1 --http-port=8391
```

Default `--address` is `0.0.0.0`. Set it explicitly.

`ord info`
----------

Prints index statistics as JSON. **It calls `index.update()` first**, so it is
a writer and cannot run alongside a server.

Fields: `blocks_indexed`, `index_path`, `index_file_size`, `stored_bytes`,
`fragmented_bytes`, `metadata_bytes`, `page_size`, `branch_pages`,
`leaf_pages`, `tree_height`, `outputs_traversed`, `sat_ranges`,
`utxos_indexed`, and `transactions`, a list of write-transaction start heights
and timestamps.

`--transactions` instead prints indexing rate: for each consecutive pair of
write transactions, the height range, the block count, and the elapsed minutes.
That is the honest measure of how fast an index is actually catching up.

```shell
ord --data-dir=/data/ord-dogecoin info --transactions
```

`ord repair-address-index`
--------------------------

Rebuilds the address index against the current unspent-output set: removes rows
for outputs that have been spent, and backfills rows for live outputs that are
missing.

| Option | Default | Meaning |
| --- | --- | --- |
| `--address-batch-size <N>` | 1000 | Addresses per committed batch. Lower means more commits and less memory. |
| `--compact` | off | Compact the database after the repair. |

The repair is resumable: progress is stored in `ADDRESS_INDEX_REPAIR_STATE`, so
an interrupted run continues where it stopped.

It prints a report: `addresses_scanned`, `live_outputs_recorded`,
`stale_outputs_removed`, `batches_committed`, `compacted`, and the index file
size before and after.

`--compact` rewrites the database file. On a large index that is a long
operation needing free space alongside the existing file. Do not start it
without both the time and the disk. See
[Operations](operations.md#repairing-the-address-index).

`ord find <SAT>`
----------------

Prints the satpoint of one ordinal. **Requires `--index-sats`**, and fails with
a message saying so otherwise. An ordinal not yet mined at the indexed height
is an error, not an empty result.

`ord list <OUTPOINT>`
---------------------

Lists the ordinal ranges in an output. **Requires `--index-sats`.**

`ord balances`
--------------

Lists all dune balances. Requires `--index-dunes`.

`ord dunes`
-----------

Lists all dunes. Requires `--index-dunes`.

`ord parse <ORDINAL>`
---------------------

Parses an ordinal from any of its notations (integer, degree, decimal, name,
percentile) and prints the integer. Does not touch the index.

`ord epochs`
------------

Prints the first ordinal of each reward epoch. Requires `STARTING_SATS_PATH`.

Dogecoin has 145,006 epochs: one per block for heights 0 to 144,999, then
epochs beginning at 145,000, 200,000, 300,000, 400,000, 500,000 and 600,000.
The output is correspondingly long.

`ord subsidy <HEIGHT>`
----------------------

Prints `first`, the starting ordinal of that block's subsidy, and `subsidy`,
its size in atomic units. A height with no subsidy is an error. Requires
`SUBSIDIES_PATH` and `STARTING_SATS_PATH`.

`ord traits <ORDINAL>`
----------------------

Prints an ordinal's traits: number, degree, height, cycle, epoch, period,
offset, rarity, name.

`ord preview <PATH>...`
-----------------------

Starts a throwaway regtest instance with the given files inscribed, for
previewing content rendering. A development tool.

`ord wallet ...`
----------------

**Non-functional against Dogecoin Core.** The subcommands require Bitcoin
Taproot descriptor wallets. See
[Differences from Bitcoin ord](differences.md#the-wallet-subcommands-do-not-work-on-dogecoin).
