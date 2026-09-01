Ord for Dogecoin
================

This section documents the software in this repository: `ord-dogecoin`, an
Ordinals indexer and explorer for the Dogecoin chain. It is the reference for
anyone who has to build it, run it, operate it, or integrate against its HTTP
API.

The rest of this handbook is the upstream Ordinal Theory Handbook, inherited
from `casey/ord`. It describes ordinal theory on **Bitcoin**. The concepts
carry over, the numbers do not: Dogecoin has a different subsidy schedule, a
different block interval, no Taproot, and no witness data. Where the two
disagree, this section is the one that describes what the code in this
repository actually does.

What this software is
---------------------

`ord-dogecoin` reads confirmed blocks from a Dogecoin Core node over JSON-RPC,
assigns ordinal numbers to Dogecoin's atomic units, tracks inscriptions
("Doginals") through the UTXO set, and optionally tracks DRC-20 token state and
Dunes token state. It stores everything in a single embedded
[redb](https://github.com/cberner/redb) file and serves it over an HTTP API and
an HTML block explorer.

It is the implementation named for the `doginals` and `drc20` protocols in the
Bitcoin Universe ecosystem registry.

**This documentation owns the implementation, not the protocol.** For the
Dogecoin protocol rules themselves, including the inscription envelope format
described as a specification rather than as a parser, see
[TAP on Doge](https://bitcoinuniverseio.github.io/tap-on-doge/). Protocol
material is not duplicated here; where this section touches protocol, it says
what the code in this repository does and points there for the rule.

What this software is not
-------------------------

- **Not a wallet you should use.** The `ord wallet` subcommands are inherited
  from upstream `ord` and gate on Bitcoin Taproot descriptors. See
  [Differences from Bitcoin ord](dogecoin/differences.md#the-wallet-subcommands-do-not-work-on-dogecoin).
- **Not a mempool service.** There is no mempool code in this repository. Every
  answer it gives is derived from confirmed blocks only.
- **Not a Dogecoin node.** It needs a fully synced Dogecoin Core node with
  `txindex=1` and never validates consensus itself.
- **Not a public multi-tenant API.** It has no authentication, no rate
  limiting, and no per-caller quotas. Put it behind something that does.
- **Not a settlement authority on its own.** It reports indexed state at a
  block height. Anything spending value must corroborate against Dogecoin Core.

Architecture
------------

```
             ┌────────────────────────────────────────────┐
             │              Dogecoin Core                 │
             │        (txindex=1, fully synced)           │
             └────────────┬───────────────────┬───────────┘
                          │ JSON-RPC          │ batched JSON-RPC
                          │ getblock          │ getrawtransaction
                          │ getblockhash      │ (nr-parallel-requests)
                          ▼                   ▼
             ┌────────────────────────────────────────────┐
             │                  Updater                   │
             │  block fetch thread ──► index_block loop   │
             │                                            │
             │  ┌──────────────────────────────────────┐  │
             │  │ InscriptionUpdater  (always)         │  │
             │  │ DuneUpdater         (--index-dunes)  │  │
             │  │ Drc20Updater        (--index-drc20)  │  │
             │  └──────────────────────────────────────┘  │
             │  commit every 1000 blocks                  │
             │  savepoint every 10 blocks near the tip    │
             └────────────────────┬───────────────────────┘
                                  │
                                  ▼
             ┌────────────────────────────────────────────┐
             │        index.redb (single redb file)       │
             │   schema version 6, feature flags fixed    │
             │   at creation time                         │
             └────────────────────┬───────────────────────┘
                                  │ read transactions
                                  ▼
             ┌────────────────────────────────────────────┐
             │            axum HTTP server                │
             │  /api/v1/*  machine contracts              │
             │  /drc20/*, /dunes/*, /inscriptions/*       │
             │  HTML explorer + /content, /preview        │
             └────────────────────────────────────────────┘
```

When `ord server` runs, the indexer is not a separate process. The server
spawns a background thread that calls `Index::update()` in a loop with a five
second sleep between passes, and serves HTTP from the same database handle.
Running `ord index` separately against the same database file is for the
initial catch-up, not for steady state alongside a server.

Where to go next
----------------

| You want to | Read |
| --- | --- |
| Understand what this fork changed | [Upstream relationship](dogecoin/upstream.md) |
| Know why Dogecoin changes the answers | [Differences from Bitcoin ord](dogecoin/differences.md) |
| Build and install it | [Installation](dogecoin/install.md) |
| Configure it | [Configuration](dogecoin/configuration.md) |
| Plan an initial sync | [Indexing](dogecoin/indexing.md) |
| Understand the stored data | [Database model](dogecoin/database.md) |
| Handle reorgs | [Reorgs and mempool](dogecoin/reorgs.md) |
| Call the API | [HTTP API](dogecoin/http-api.md) |
| Use the CLI | [CLI reference](dogecoin/cli.md) |
| Run it in production | [Operations](dogecoin/operations.md) |
| Fix something broken | [Troubleshooting](dogecoin/troubleshooting.md) |
| Size a machine | [Performance and sizing](dogecoin/performance.md) |
| Review the threat model | [Security](dogecoin/security.md) |
| Run the tests | [Testing](dogecoin/testing.md) |
| Ship a change | [Releases and versioning](dogecoin/releases.md) |
