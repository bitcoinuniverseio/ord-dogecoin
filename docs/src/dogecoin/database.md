Database model
==============

There is one file. `index.redb` is an embedded
[redb](https://github.com/cberner/redb) key-value store, version 2.4.0, holding
every table below. There is no external database, no schema migration tool, and
no way to add a feature to an existing file.

Schema version
--------------

`SCHEMA_VERSION` is **6**, stored in `STATISTIC_TO_COUNT` under the `Schema`
key and checked on every open:

| Condition | Behaviour |
| --- | --- |
| Stored version is lower | Refuses to open: "index ... appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index". |
| Stored version is higher | Refuses to open: "... consider updating ord". |
| Equal | Opens. |

There is no in-place upgrade path. A schema bump means a rebuild, or restoring
a snapshot built at the new version.

The redb major version matters too: this build uses redb 2.4.0, which is not
file-compatible with databases written by `wonky-ord` builds on older redb
releases.

Feature flags are part of the file
----------------------------------

`Statistic::IndexSats`, `IndexTransactions`, `IndexDrc20` and `IndexDunes` are
written into `STATISTIC_TO_COUNT` **when the database is created** and read
back on every later open. `Index::open` uses the stored values, not the command
line.

This is the single most common source of confusion with this software:

- Adding `--index-drc20` to an existing database does nothing and prints no
  warning.
- Removing it from a database that has it does nothing either.
- The historical protocol state was never parsed, so no flag can backfill it.

`GET /api/v1/capabilities` reports the stored values. Use it, not your systemd
unit, as the answer to "what can this index do".

Tables
------

### Core chain and inscription state

| Table | Key | Value | Written when |
| --- | --- | --- | --- |
| `HEIGHT_TO_BLOCK_HASH` | height | block hash | always |
| `INSCRIPTION_ID_TO_INSCRIPTION_ENTRY` | inscription id | fee, height, inscription number, ordinal (only with `--index-sats`), sequence number, timestamp | always |
| `INSCRIPTION_ID_TO_SATPOINT` | inscription id | satpoint | always |
| `INSCRIPTION_NUMBER_TO_INSCRIPTION_ID` | number | inscription id | always |
| `SATPOINT_TO_INSCRIPTION_ID` | satpoint | inscription id | always |
| `SAT_TO_INSCRIPTION_ID` | ordinal | inscription id | always |
| `SAT_TO_SATPOINT` | ordinal | satpoint | always |
| `OUTPOINT_TO_VALUE` | outpoint | value in atomic units | always |
| `STATISTIC_TO_COUNT` | statistic id | counter | always |
| `WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP` | height | epoch milliseconds | always |

### Multi-transaction inscription assembly

| Table | Purpose |
| --- | --- |
| `PARTIAL_TXID_TO_INSCRIPTION_TXIDS` | An inscription envelope split across several transactions, still incomplete. |
| `INSCRIPTION_ID_TO_TXIDS` | The transaction chain that assembled a completed inscription. |
| `INSCRIPTION_TXID_TO_TX` | The raw transactions of that chain, needed to reassemble the content. |

These three have no equivalent in Bitcoin `ord`. They exist because Dogecoin
inscriptions are chained through `scriptSig` rather than carried in one witness.
See [Differences from Bitcoin ord](differences.md#inscriptions-live-in-scriptsig-not-in-a-witness).

### Address index

| Table | Key | Value |
| --- | --- | --- |
| `OUTPOINT_TO_ADDRESS` | outpoint | script or address bytes |
| `ADDRESS_TO_OUTPOINT` | address bytes | outpoints (multimap) |
| `ADDRESS_INDEX_REPAIR_STATE` | marker | resumable repair cursor |

The address index is what makes `/address/:address`, `/utxos/balance/:address`,
`/inscriptions/balance/:address`, `/drc20/balance/:address` and
`/api/v1/funding/:address` possible. It can accumulate rows for outputs that
were later spent; `ord repair-address-index` removes stale rows and backfills
missing live ones. See
[Operations](operations.md#repairing-the-address-index).

### Ordinal ranges, with `--index-sats`

| Table | Purpose |
| --- | --- |
| `OUTPOINT_TO_SAT_RANGES` | The ordinal ranges held by each unspent output. |

This is the expensive one. It is what makes `ord find`, `/sat/:sat`,
`/range/:start/:end`, `/rare.txt` and rarity work, and it multiplies the size of
the index. Enable it only if you need ordinal-level queries.

### Transactions, with `--index-transactions`

| Table | Purpose |
| --- | --- |
| `TRANSACTION_ID_TO_TRANSACTION` | Raw transaction bytes. |

Required by DRC-20 indexing and by `/api/v1/funding/:address`, which returns
the raw previous transaction for each cardinal output so a caller can verify
the prevout independently.

### Dunes, with `--index-dunes`

| Table | Purpose |
| --- | --- |
| `DUNE_ID_TO_DUNE_ENTRY` | Etching, terms, supply, mints, burned. |
| `DUNE_TO_DUNE_ID` | Name to id. |
| `OUTPOINT_TO_DUNE_BALANCES` | Per-output dune balances. |
| `INSCRIPTION_ID_TO_DUNE` | Dune etched by an inscription. |
| `TRANSACTION_ID_TO_DUNE` | Dune etched by a transaction. |

`Statistic::Dunes` and `Statistic::ReservedDunes` count etched and reserved
dunes.

### DRC-20, with `--index-drc20`

| Table | Purpose |
| --- | --- |
| `DRC20_TOKEN` | Deployment: ticker, decimals, max, limit, minted, deployer, height. |
| `DRC20_BALANCES` | Per-holder overall and available balances. |
| `DRC20_TRANSFERABLELOG` | Outstanding transferable inscriptions. |
| `DRC20_INSCRIBE_TRANSFER` | Inscribe-transfer operations awaiting their transfer. |
| `DRC20_TOKEN_HOLDER` | Ticker to holders (multimap), which is what `holder_count` counts. |

DRC-20 tickers are exactly four bytes (`TICK_BYTE_COUNT = 4`) and are matched
case-insensitively through a lowercase form. The protocol literal is `drc-20`,
and an operation is only considered when the inscription content type starts
with `text/plain` or `application/json` and the body is at least 40 bytes.
Maximum decimal width is 18.

Statistics
----------

`STATISTIC_TO_COUNT` also holds running counters exposed by `ord info`:

| Statistic | Meaning |
| --- | --- |
| `Commits` | Write transactions committed. |
| `LostSats` | Atomic units that left the tracked set. |
| `OutputsTraversed` | Outputs processed since creation. |
| `SatRanges` | Ordinal ranges stored. |
| `Dunes`, `ReservedDunes` | Dune counters. |

Consistency and durability
--------------------------

- One writer at a time. redb enforces this at the file level. `ord server`,
  `ord index` and `ord info` are all writers.
- Commits happen every 1000 blocks and at graceful shutdown. Everything since
  the last commit is lost on a crash and simply reindexed.
- Persistent savepoints, taken every 10 blocks near the tip, are the reorg
  rollback mechanism, not a backup. See [Reorgs and mempool](reorgs.md).
- **A file copy of an open database is not a backup.** redb has no external
  consistent-snapshot tool. See [Operations](operations.md#backups).
