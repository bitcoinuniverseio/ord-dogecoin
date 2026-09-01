# ord-dogecoin

Ordinals indexer, HTTP API and block explorer for **Dogecoin**. It assigns
ordinal numbers to Dogecoin's atomic units, tracks inscriptions (Doginals)
through the UTXO set, and optionally indexes DRC-20 and Dunes token state.

This is the Bitcoin Universe fork of
[Trac-Systems/ord-dogecoin](https://github.com/Trac-Systems/ord-dogecoin),
which descends from
[verydogelabs/wonky-ord-dogecoin](https://github.com/verydogelabs/wonky-ord-dogecoin),
[apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) and
[casey/ord](https://github.com/casey/ord). Licensed CC0-1.0, like everything
upstream of it.

| | |
| --- | --- |
| **Chain** | Dogecoin mainnet |
| **Protocols** | Doginals (Ordinals), DRC-20, Dunes |
| **Requires** | Dogecoin Core with `txindex=1`, fully synced |
| **Storage** | One embedded redb file, schema version 6 |
| **Language** | Rust 1.88 or newer |
| **Lifecycle** | Experimental. No Universe release; the `1.0.x` tags are upstream's. |

## Documentation

Full documentation lives in [`docs/src/dogecoin/`](docs/src/dogecoin) and
builds into this repository's mdbook.

| | |
| --- | --- |
| [Overview and architecture](docs/src/dogecoin.md) | What it is, what it is not, how the pieces fit |
| [Upstream relationship](docs/src/dogecoin/upstream.md) | Lineage, license, what this fork changed |
| [Differences from Bitcoin ord](docs/src/dogecoin/differences.md) | Why Dogecoin changes the answers |
| [Installation](docs/src/dogecoin/install.md) | Build, Docker, systemd, requirements |
| [Configuration](docs/src/dogecoin/configuration.md) | Every option and environment variable |
| [Indexing](docs/src/dogecoin/indexing.md) | Initial and incremental sync |
| [Database model](docs/src/dogecoin/database.md) | Tables, schema version, feature flags |
| [Reorgs and mempool](docs/src/dogecoin/reorgs.md) | Savepoints, recovery limits, no mempool |
| [HTTP API](docs/src/dogecoin/http-api.md) | Endpoint reference |
| [CLI reference](docs/src/dogecoin/cli.md) | Every subcommand |
| [Operations](docs/src/dogecoin/operations.md) | Health, alerting, backups, upgrades |
| [Performance and sizing](docs/src/dogecoin/performance.md) | Disk, RAM, RPC concurrency |
| [Troubleshooting](docs/src/dogecoin/troubleshooting.md) | Organized by symptom |
| [Security](docs/src/dogecoin/security.md) | Threat model and hardening |
| [Testing](docs/src/dogecoin/testing.md) | What is covered, and what is not |
| [Releases and versioning](docs/src/dogecoin/releases.md) | Branches, CI, compatibility rules |

The HTTP contract is [`openapi.yaml`](openapi.yaml), an OpenAPI 3.0 document
covering every route the server exposes. Load it into any OpenAPI tool.

This repository documents the **implementation**. For the Dogecoin protocol
rules themselves, including the inscription envelope described as a
specification, see
[TAP on Doge](https://bitcoinuniverseio.github.io/tap-on-doge/).

## Quick start

### 1. Run a Dogecoin node

```shell
dogecoind -txindex -rpcuser=YOUR_USER -rpcpassword=YOUR_PASSWORD \
          -rpcport=22555 -rpcbind=127.0.0.1
```

`txindex=1` is required. Let the node finish syncing before you start the
indexer. Dogecoin Core 1.14.8 and 1.14.9 are the versions this fork has been
run against.

### 2. Build

```shell
cargo build --locked --release --bin ord
```

Use `--locked`: the build patches `bitcoin` and `bitcoincore-rpc` to Dogecoin
forks, and an unlocked resolve can pick incompatible versions.

### 3. Set the two mandatory environment variables

```shell
export SUBSIDIES_PATH=$PWD/subsidies.json
export STARTING_SATS_PATH=$PWD/starting_sats.json
export RUST_LOG=info
```

Dogecoin's first 145,000 blocks paid randomized rewards, so the subsidy
schedule cannot be computed and is loaded from these files. **The process
panics without them.** Keep them with the binary: two indexes built from
different files disagree about ordinal numbers.

### 4. Index

```shell
# Inscriptions only
ord --rpc-url=http://USER:PASSWORD@127.0.0.1:22555 \
    --data-dir=/data/ord-dogecoin \
    --first-inscription-height=4609723 \
    --nr-parallel-requests=16 \
    index

# Full index: inscriptions, transactions, DRC-20 and Dunes
ord --rpc-url=http://USER:PASSWORD@127.0.0.1:22555 \
    --data-dir=/data/ord-dogecoin \
    --first-inscription-height=4609723 \
    --first-dune-height=5084000 \
    --nr-parallel-requests=16 \
    --index-transactions --index-drc20 --index-dunes \
    index
```

> **The index feature flags are fixed when the database is created.** Adding
> `--index-drc20` later does nothing and prints no warning, because the
> historical protocol state was never parsed. Decide the feature set before the
> first run.

An index from scratch takes days. See
[Indexing](docs/src/dogecoin/indexing.md).

### 5. Serve

```shell
ord --rpc-url=http://USER:PASSWORD@127.0.0.1:22555 \
    --data-dir=/data/ord-dogecoin \
    server --http --address=127.0.0.1 --http-port=8080
```

`ord server` runs the indexer **and** the HTTP server in one process. The
default `--address` is `0.0.0.0`; set it explicitly and put a reverse proxy in
front. There is no authentication and no rate limiting.

Check what the index can answer:

```shell
curl -s http://127.0.0.1:8080/api/v1/capabilities
```

```json
{"chain":"dogecoin","block_count":5764321,"block_hash":"...",
 "drc20":true,"dunes":true,"sats":false,"transactions":true}
```

## Docker

```shell
cp .env.example .env      # then edit RPC_URL and the rest
docker build -t ord-dogecoin .
docker compose up -d
```

**Always stop with a long timeout.** redb must close the database cleanly, and
a hard kill leaves a database that takes a very long time to reopen:

```shell
docker compose stop -t 600
docker compose down
```

## Starting from a pre-indexed database

Upstream publishes a pre-indexed redb file of roughly 300 GB at
`https://legacy.trac.network/doginals-nodrc20-nodunes-redb220.redb`. It is
**inscriptions only**: no DRC-20 and no Dunes data. It is a third-party
artifact that nothing in this repository verifies, and the parent `wonky-ord`
client cannot read it, because this fork uses redb 2.4.0.

```shell
ord --rpc-url=http://USER:PASSWORD@YOUR_NODE:22555 \
    --first-inscription-height=4609723 \
    --nr-parallel-requests=16 \
    --index=/path/to/doginals-nodrc20-nodunes-redb220.redb \
    index
```

Confirm what you actually got with `/api/v1/capabilities` before trusting it.

## Dogecoin is not Bitcoin

If you have run `ord` on Bitcoin, these are the differences that will bite you:

- **One minute blocks.** A confirmation here is worth roughly a tenth of a
  Bitcoin confirmation in elapsed work. Six confirmations is about six minutes.
- **No Taproot, no witness.** Inscriptions live in `scriptSig` and large ones
  are chained across consecutive transactions.
- **No content size limit**, on any network. Doginals are routinely far larger
  than Bitcoin inscriptions, and the index grows accordingly.
- **The subsidy schedule is a data file**, not a formula, because Dogecoin's
  first 145,000 blocks paid randomized rewards. This repository keeps the real
  per-block rewards; see `src/epoch.rs`.
- **Base58 addresses only**, with Dogecoin's version bytes.
- **`ord wallet` does not work.** It requires Bitcoin Taproot descriptor
  wallets, which Dogecoin Core cannot produce. Treat it as inherited dead code.
- **No mempool.** Everything served is derived from confirmed blocks only.

The full list, with the code that implements each one, is in
[Differences from Bitcoin ord](docs/src/dogecoin/differences.md).

## Development

```shell
cargo clippy --locked -p ord-dogecoin --lib --bin ord \
  --test compatibility --test authority-api-contract --all-features
cargo test --locked --test compatibility
cargo test --locked --test authority-api-contract
./bin/forbid
```

CI runs exactly this on Linux and Windows, plus a release build, a CLI smoke
test, and an mdbook build of `docs/`.

Only `tests/compatibility.rs` and `tests/authority_api_contract.rs` are Cargo
test targets. The rest of `tests/` is upstream fixture code that does not
compile against this Dogecoin snapshot and is retained for incremental repair,
not represented as passing. See [Testing](docs/src/dogecoin/testing.md).

## Contributing, support and security

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [SUPPORT.md](SUPPORT.md)
- [SECURITY.md](SECURITY.md) for vulnerability reports. Do not open a public
  issue.

## License

[CC0-1.0](LICENSE). Public domain dedication, no warranty.

The upstream Ordinal Theory Handbook that ships in `docs/` and
[`READMEFROMAPEZORD.md`](READMEFROMAPEZORD.md) are retained from upstream and
describe ordinal theory on Bitcoin.
