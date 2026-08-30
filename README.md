# Shibes
# Trac & Magic Degen

TracSystems, operating "Magic Degen" for the Dogecoin ecosystem, offers a suite of repositories designed to provide seamless, secure, and decentralized tracking solutions. These repositories are tailored to the unique requirements of Dogecoin, enabling developers to integrate advanced tracking functionalities into their applications.

To safe you a lot of indexing time, we provided a download for a pre-indexed redb file. The parent wonky-ord ord client won't work, because this fork is using redb 2.4.0, which is incompatible.

The use of the latest Redb version introduces massive speed and stability improvements over previous versions.

Please [download the file here](https://legacy.trac.network/doginals-nodrc20-nodunes-redb220.redb) (approx. 300GB) and follow the instructions below to start the index.

**WARNING**: this file is only for a plain inscription index and does NOT include DRC-20 and Dunes indexed data!

ℹ️ This is a fork/based on [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin)

## Key differences

‼️ DISCLAIMER: OUR CODE MAY STILL HAVE BUGS️

We included the real wonky block rewards from block 0 until block 144,999. We invite you to critically review our code in `src/epoch.rs`. We are convinced that doginals should use actual block rewards instead of a simplified version.

## API documentation
You can find the API documentation [here](openapi.yaml).
Most convenient way to view the API documentation is to use the [Swagger Editor](https://editor.swagger.io/).
You can import the `openapi.yaml` file and view the API documentation via Import URL: `https://raw.githubusercontent.com/verydogelabs/wonky-ord-dogecoin/main/openapi.yaml`.

### Universe authority inventory

`GET /api/v1/funding/{address}?limit=20` returns confirmed cardinal UTXOs for an authoritative Dogecoin address. Each item includes the exact atomic value, script, confirmations, and raw previous transaction so downstream services can independently verify the prevout. Outputs carrying inscriptions or Dunes are excluded. The endpoint requires `--index-transactions`; callers must apply any additional protocol reservations owned by downstream indexers before treating an output as spendable. `limit` is bounded to 1–50. `total_count` reports the complete cardinal UTXO count before the response bound and `truncated` states whether additional entries exist; `inventory_complete` describes index completeness and must not be interpreted as an unbounded page.

`GET /api/v1/drc20/tokens?cursor=0&limit=250` returns the DRC-20 deployment catalog with its indexed protocol state: ticker, deploy inscription id and number, decimals, `max_atomic`, `limit_atomic`, `minted_atomic`, `remaining_atomic`, `holder_count`, deployment height and timestamp, deployer, latest mint number, and whether minting is complete. This is deliberately not the transferable inventory: a valid deployment with no outstanding transferable still appears here, so a downstream token index built on this endpoint cannot silently drop real tokens. `limit` is bounded to 1-1000 and `cursor` is a decimal offset over a deterministic ticker order, so a page boundary can neither repeat nor skip a token at a given indexed height. Every quantity is a string, so a `u128` supply is never rounded through a JSON number.

`GET /api/v1/drc20/tokens/{tick}/holders?cursor=0&limit=250` returns holder balances for one ticker in atomic units, split into `overall_atomic`, `transferable_atomic`, and `available_atomic`, with the same bounds and cursor semantics.

Both endpoints require an index built with `--index-drc20`. That flag is recorded when the index is created and cannot add missing historical protocol state to an existing database; an index built without it reports an empty catalog.

`GET /api/v1/dunes/tokens?cursor=0&limit=250` returns the dune catalog with its indexed protocol state: the spaced name, the `block:index` identifier of the etching, number, symbol (null when none was etched), divisibility, etching txid, `supply_atomic`, `premine_atomic`, `mints_atomic`, `burned_atomic`, etched height and timestamp, and whether the terms allow a mint in the next block. Every quantity is a string, so a `u128` supply is never rounded through a JSON number, and `divisibility` is the only rule by which one may be scaled. The same limit bounds and deterministic offset-cursor semantics apply, ordered by etching.

`GET /api/v1/dunes/tokens/{dune}` returns one dune by spaced name or `block:index` identifier, with the same item contract.

Both dune endpoints require an index built with `--index-dunes` and refuse to answer from a database created without it, for the same reason the DRC-20 endpoints do.

`GET /api/v1/capabilities` reports the chain, the indexed checkpoint, and whether this database can answer DRC-20, Dunes, sat and transaction queries. It exists because index feature flags are recorded once, when the database is created, and read back on every later open: adding `--index-drc20` or `--index-transactions` to the startup command afterwards does nothing and cannot backfill historical protocol state. Without an explicit capability report, a database that never indexed DRC-20 answers every DRC-20 query with `200 []`, which downstream is indistinguishable from a chain that genuinely has no tokens. Consumers should also use it to confirm they are talking to the Dogecoin index rather than another chain's ord instance on a neighbouring port.

The DRC-20 and funding endpoints fail closed with an actionable message when the corresponding capability is absent, rather than returning an empty result, and the DRC-20 payloads carry `drc20_index_enabled` so an empty catalog is never ambiguous.

## Continuous verification

CI validates the locked Dogecoin dependency graph, formatting, Clippy, and the maintained protocol compatibility suite in `tests/compatibility.rs` on Linux, macOS, and Windows. The suite exercises Dune identifiers plus Dunestone script encoding and decoding against the pinned `rust-dogecoin` API.

The historical upstream fixture corpus is retained for reference but is not a Cargo test target because it mixes incompatible Bitcoin-era APIs and Bitcoin reward assumptions with this Dogecoin fork. Add new coverage to the compatibility suite or migrate a fixture before making it required in CI.

## TL;DR How to run

### Preqrequisites
You will have to launch your own Dogecoin node and have it fully synced. You can use the following guide to set up your own Dogecoin node:
1. Download latest version from [Dogecoin](https://github.com/dogecoin/dogecoin/releases) and install it.
   1. We have tested and launched the indexer with Dogecoin Core v1.14.8.
2. Follow the [installation instructions](https://github.com/dogecoin/dogecoin/blob/master/INSTALL.md)
   1. We started the Dogecoin Core with the following flags:
      ```shell
      dogecoind -txindex -rpcuser=foo -rpcpassword=bar -rpcport=22555 -rpcallowip=0.0.0.0/0 -rpcbind=127.0.0.1
      ```
   2. Make sure your Dogecoin node is fully synced before starting the indexer.
   3. ‼️ **IMPORTANT** Ensure to replace `foo` and `bar` with your own username and password. **IMPORTANT** ‼️
3. Start the indexer with rpc-url pointing to your Dogecoin node and the data-dir pointing to the directory where the indexer should store its data.

```shell

### Start the ord indexer / server
```shell
export RUST_LOG=info
// Set the path to the subsidies.json and starting_sats.json files
export SUBSIDIES_PATH=/home/dogeuser/wonky-ord-dogecoin/subsidies.json
export STARTING_SATS_PATH=/home/dogeuser/wonky-ord-dogecoin/starting_sats.json

# ensure the data directory exists
mkdir -p /mnt/ord-node/indexer-data-main

# replace YOUR_RPC_URL with the URL of your Dogecoin node like: http://foo:bar@127.0.0.1:22555

// WITH PRE-INDEXED FILE (no drc20, no dunes, just inscriptions, see download above)

// Start Indexing
ord --rpc-url=http://foo:bar@YOURIP:25555 --first-inscription-height=4609723 --nr-parallel-requests=16 --index=/path/to/doginals-nodrc20-nodunes-redb220.redb index

// Start Indexing + Server
ord --rpc-url=http://foo:bar@YOURIP:25555 --first-inscription-height=4609723 --nr-parallel-requests=16 --index=/path/to/doginals-nodrc20-nodunes-redb220.redb server --address YOURIP --http-port YOURPORT

// WITHOUT PRE-INDEXED FILE (from scratch, can take many days)

// Start Indexing
ord --rpc-url=YOUR_RPC_URL --data-dir=/mnt/ord-node/indexer-data-main --nr-parallel-requests=16 --first-inscription-height=4609723 --first-dune-height=5084000 --index-dunes --index-transactions --index-drc20 index

// Start Indexing + Server
ord --rpc-url=YOUR_RPC_URL --data-dir=/mnt/ord-node/indexer-data-main --nr-parallel-requests=16 --first-inscription-height=4609723 --first-dune-height=5084000 --index-dunes --index-transactions --index-drc20 server
```
`--index-transactions` will store transaction data, this is currently needed for `--index-drc20` and furthermore helps
for a better performance for the API.
`--nr-parallel-requests` will configure how many parallel requests while indexing are sent to your RPC Server - 16 is
recommended for default node settings.

With all settings enabled, the database will currently need around 400gb when fully indexed.

### Required env vars

On the root level of this repo you'll find a `subsidies.json` and `starting_sats.json` file. When starting ord you will need to set the location of these files to env variables.

Example:
If your `wonky-ord-dogecoin` dir is `/home/dogeuser/wonky-ord-dogecoin` then set the vars:
`SUBSIDIES_PATH=/home/dogeuser/wonky-ord-dogecoin/subsidies.json`
and
`STARTING_SATS_PATH=/home/dogeuser/wonky-ord-dogecoin/starting_sats.json`.

## Start the ord indexer / server in Docker
You can use a docker image to run the ord indexer / server.

### Prerequisites Docker
1. Use ubuntu linux or a similar distribution
2. Install dogecoind and have it fully synced
   1See [Dogecoin installation instructions](#preqrequisites)
3. Install docker and docker-compose (Ubuntu)[https://docs.docker.com/engine/install/ubuntu/]
4. Clone this repository

### Build the Docker image
```shell
docker build -t verydogelabs/wonky-ord-dogecoin .
```
### Start the ord in a docker container
```shell
docker compose up -d
```

### Stop the ord in a docker container
When stopping the ord in a container it is important to add a timeout.
If no timeout is add, the process cannot close the database properly and the next start will take ages or fail.

```shell
docker compose stop -t 600
docker compose down
```

## Original README
Please check the original [README](READMEFROMAPEZORD.md) for more information on how to run `ord` and the required env vars.
