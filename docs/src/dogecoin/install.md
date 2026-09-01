Installation
============

System requirements
-------------------

**Dogecoin Core.** A fully synced node with `txindex=1`. The reference
deployment in `deploy/dogecoin.conf` uses:

```
datadir=/var/lib/dogecoind
maxmempool=1024
txindex=1
```

`txindex=1` is not optional: the indexer calls `getrawtransaction` for
arbitrary historical transactions. Dogecoin Core 1.14.8 and 1.14.9 are the
versions this fork has been run against.

**Rust.** 1.88 or newer. `Cargo.toml` sets `rust-version = "1.88"` and the
pinned dependency graph in `Cargo.lock` will not build on older toolchains.

**Build host.** A C toolchain, `pkg-config` and OpenSSL headers. On Debian or
Ubuntu:

```shell
sudo apt-get install -y build-essential pkg-config libssl-dev ca-certificates curl git
```

**Machine.** See [Performance and sizing](performance.md) for the numbers. The
short version for a full mainnet index with DRC-20, Dunes and transactions
enabled: 8 or more cores, 16 GiB of RAM, and roughly 500 GiB of fast NVMe with
headroom to grow.

Building from source
--------------------

```shell
git clone https://github.com/bitcoinuniverseio/ord-dogecoin.git
cd ord-dogecoin
cargo build --locked --release --bin ord
./target/release/ord --version
```

Use `--locked`. The build patches `bitcoin` and `bitcoincore-rpc` to
Dogecoin forks via `[patch.crates-io]` in `Cargo.toml`, and an unlocked
resolve can pick incompatible versions.

The binary is named `ord`, not `ord-dogecoin`. If you also run Bitcoin `ord` on
the same host, install them under distinct paths.

Docker
------

The repository ships a two-stage `Dockerfile` and a `docker-compose.yaml`.

```shell
docker build -t ord-dogecoin .
```

The image copies `subsidies.json` and `starting_sats.json` to `/subsidies.json`
and `/starting_sats.json`, and `docker-compose.yaml` sets `SUBSIDIES_PATH` and
`STARTING_SATS_PATH` to match. If you write your own compose file, keep those
two variables set. See
[Required environment variables](configuration.md#required-environment-variables).

Copy `.env.example` to `.env` and edit it before starting:

```
RUST_LOG=info
RPC_URL=http://foo:bar@127.0.0.1:22555
FIRST_INSCRIPTION_HEIGHT=4609723
FIRST_DUNE_HEIGHT=5084000
ORD_HTTP_PORT=8080
DOG_MOUNT_DIR_INDEXER=/mnt/ord-node
COMPOSE_PROJECT_NAME=ord-indexer
```

Replace `foo:bar` with your own node credentials, and do not commit `.env`.

```shell
docker compose up -d
```

**Always stop with a long timeout.** redb needs to close the database cleanly,
and a `SIGKILL` mid-commit leaves a database that takes a very long time to
reopen, or fails to reopen at all:

```shell
docker compose stop -t 600
docker compose down
```

The compose file uses `network_mode: host` so the container can reach a
Dogecoin node listening on the host's loopback address.

systemd
-------

`deploy/linux/universe-ord-dogecoin-full.service` is a working unit for a full
index. The parts worth copying into any unit you write:

| Directive | Why |
| --- | --- |
| `KillSignal=SIGINT` | The process installs a handler (the `ctrlc` crate with the `termination` feature, so `SIGINT`, `SIGTERM` and `SIGHUP` all reach it) that sets a shutdown flag, stops the HTTP listeners, and lets the index thread finish its current block and commit. A **second** signal calls `process::exit(1)` immediately, so do not send one twice. |
| `SendSIGKILL=no` | Never hard-kill a redb writer. |
| `TimeoutStopSec=30min` | A commit in flight can take a long time on a large index. |
| `TimeoutStartSec=10min` | Opening a large database is not instant. |
| `Restart=on-failure` with `RestartSec=30` | Restart loops against a busy node make things worse. |
| `LimitNOFILE=65536` | redb plus many parallel RPC sockets. |
| `Environment=SUBSIDIES_PATH=...` and `STARTING_SATS_PATH=...` | Mandatory. The process panics without them. |
| `MemoryHigh` / `MemoryMax` above the redb cache size | The `--db-cache-size` value is a floor for resident memory, not a cap on the process. |

The unit also applies the standard systemd hardening set
(`ProtectSystem=strict`, `NoNewPrivileges=true`, `PrivateTmp=true`,
`RestrictAddressFamilies=`, and a dedicated service user). Keep it. See
[Security](security.md).

Verifying an installation
-------------------------

```shell
ord --version
ord --rpc-url=http://user:pass@127.0.0.1:22555 --data-dir=/var/lib/ord-dogecoin info
```

`ord info` prints the index statistics described in
[CLI reference](cli.md#ord-info). Against a fresh data directory it creates the
database, so run it with the same flags you intend to use permanently: the
index feature flags are fixed at creation time and cannot be changed
afterwards.
