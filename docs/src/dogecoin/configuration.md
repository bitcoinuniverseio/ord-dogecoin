Configuration
=============

There are three configuration surfaces: environment variables, global CLI
options, and an optional YAML file. There is no configuration file that can set
the index flags, and none of the index flags can be changed after the database
is created.

Required environment variables
------------------------------

| Variable | Required | Meaning |
| --- | --- | --- |
| `SUBSIDIES_PATH` | **Yes** | Absolute path to `subsidies.json`, the epoch-to-subsidy table. |
| `STARTING_SATS_PATH` | **Yes** | Absolute path to `starting_sats.json`, the first ordinal number of each epoch. |

Both files ship in the repository root. Both are read with `expect()`, so a
missing variable or unreadable file **panics the process** the first time a
subsidy or starting ordinal is needed, which in practice is during the first
block it indexes.

```shell
export SUBSIDIES_PATH=/opt/ord-dogecoin/subsidies.json
export STARTING_SATS_PATH=/opt/ord-dogecoin/starting_sats.json
```

These two files define ordinal numbering. Two indexes built from different
copies will disagree about which ordinal a given inscription sits on. Keep them
with the binary and treat them as part of the release artifact.

Optional environment variables
------------------------------

| Variable | Read by | Meaning |
| --- | --- | --- |
| `RUST_LOG` | `env_logger` | Log level. `info` is the useful floor: at `info` the progress bar is suppressed and per-block progress is logged instead. `debug` is very noisy on a full index. |
| `RUST_BACKTRACE` | `src/lib.rs` | When set to exactly `1`, a failing command prints the anyhow backtrace after the error chain. |
| `ORD_INTEGRATION_TEST` | `src/lib.rs` | Test-only. When set to a non-empty value, forces `first_inscription_height` and `first_dune_height` to 0. Never set this in production: it makes the indexer scan the whole chain for envelopes. |

`docker-compose.yaml` additionally reads `RPC_URL`, `FIRST_INSCRIPTION_HEIGHT`,
`FIRST_DUNE_HEIGHT`, `ORD_HTTP_PORT`, `DOG_MOUNT_DIR_INDEXER` and
`COMPOSE_PROJECT_NAME` from `.env`. Those are compose-level substitutions that
build the command line. The binary itself does not read them.

Global options
--------------

These come before the subcommand: `ord <global options> <subcommand>`.

### Connection

| Option | Default | Notes |
| --- | --- | --- |
| `--rpc-url <URL>` | `127.0.0.1:<chain port>/<wallet>` | Dogecoin Core JSON-RPC endpoint. May embed credentials as `http://user:pass@host:port`. |
| `--cookie-file <PATH>` | `<dogecoin data dir>/.cookie` | Used when it exists. When it does not, credentials are parsed from `--rpc-url` instead. |
| `--dogecoin-data-dir <PATH>` | `~/.dogecoin` on Linux, the platform data directory elsewhere | Only used to locate the cookie file. |

Authentication precedence is: if the cookie file exists, use it; otherwise use
the username and password embedded in `--rpc-url`. Prefer the cookie file. A
URL with credentials in it shows up in `ps`, in shell history, and in systemd
unit files.

The client refuses to start when the node reports a different chain than the
`--chain` value, with `Dogecoin RPC server is on <x> but ord is on <y>`.

### Storage

| Option | Default | Notes |
| --- | --- | --- |
| `--data-dir <PATH>` | Platform data directory, `ord` subdirectory | The database is created at `<data-dir>/index.redb`. Non-mainnet chains append a subdirectory. |
| `--index <PATH>` | `<data-dir>/index.redb` | Point directly at a database file. Takes precedence over `--data-dir`. Useful when restoring a snapshot with a different filename. |
| `--db-cache-size <BYTES>` | One quarter of total system memory | redb page cache. This is resident memory the process will hold, so set the cgroup or systemd memory limit above it. |

### Index features (fixed at creation)

| Option | Effect |
| --- | --- |
| `--index-sats` | Track the location of every atomic unit. Very expensive. Required for `/sat`, `/range`, rarity and `ord find`. |
| `--index-transactions` | Store raw transactions in the index. Required by `--index-drc20` and by `/api/v1/funding/:address`. |
| `--index-drc20` | Track DRC-20 deploys, mints, transfers and balances. |
| `--index-dunes` | Track Dunes etchings, edicts and balances. |

**These four flags are written into the database when it is created and read
back on every later open.** Passing a flag to an existing database does
nothing, silently. There is no backfill: the historical protocol state was
never parsed, so it cannot be recovered without a rebuild. `ord` does not warn
you about this, which is why `/api/v1/capabilities` exists.

Decide the feature set before the first run. If you are not sure, enable
`--index-transactions --index-drc20 --index-dunes`; the marginal cost is much
lower than a rebuild.

### Scan window

| Option | Default (mainnet) | Notes |
| --- | --- | --- |
| `--first-inscription-height <H>` | 4,600,000 | Below this height, and without `--index-sats`, only block headers are fetched. Lowering it after the fact does not rescan. |
| `--first-dune-height <H>` | 5,084,000 | First height at which dunestones are parsed. |
| `--height-limit <H>` | none | Stop indexing at this height. Used by the chunked migration tooling in `deploy/aws-doge-index/`. |

The reference deployments use `--first-inscription-height=4609723`, which is
above the chain default. That is the height of the first known Doginal in
production use. The lower default of 4,600,000 is safe but scans about 9,700
extra blocks.

### Throughput

| Option | Default | Notes |
| --- | --- | --- |
| `--nr-parallel-requests <N>` | 12 | Concurrent `getrawtransaction` calls batched into a single JSON-RPC POST while indexing. |

Higher is not always faster. The value has to fit the node's `rpcthreads` and
`rpcworkqueue`; too high and Dogecoin Core starts rejecting or queueing
requests, and the indexer's retry backoff makes throughput worse. `deploy/`
uses 4 during catch-up against a node that is also serving other traffic, 16 is
the upstream recommendation for a dedicated node with default settings, and
`docker-compose.yaml` ships 250, which is only reasonable against a node tuned
for it.

### Chains and networks

| Option | Notes |
| --- | --- |
| `--chain <mainnet\|testnet\|signet\|regtest>` | Default `mainnet`. `main` and `test` are accepted aliases. |
| `--testnet` / `-t`, `--regtest` / `-r`, `--signet` / `-s` | Shorthands. Mutually exclusive with `--chain`. |

| Chain | Default RPC port | Default first inscription height | Data dir suffix |
| --- | --- | --- | --- |
| `mainnet` | 22555 | 4,600,000 | none |
| `testnet` | 44555 | 4,250,000 | `testnet3` |
| `regtest` | 18332 | 0 | `regtest` |
| `signet` | 38332 | 0 | `signet` |

Only **mainnet** is run and verified by this fork, and `docs.manifest.json`
declares mainnet only. `testnet` and `regtest` carry Dogecoin values and should
work, but are not exercised in CI. `signet` is inherited from upstream `ord`;
Dogecoin has no signet, and the entry carries Bitcoin's port and Bitcoin's
data-directory layout. Do not use it.

### Explorer and content

| Option | Notes |
| --- | --- |
| `--csp-origin <ORIGIN>` | Sets the `Content-Security-Policy` origin for served inscription content. Set it to the public-facing URL of the instance. Without it the header is `default-src 'self'`, which is relative to the origin the content is served from. See [Security](security.md#serving-inscription-content). |
| `--config <PATH>` | Load a YAML config file. |
| `--config-dir <PATH>` | Load `<dir>/ord.yaml` if it exists. |
| `--wallet <NAME>` | Default `ord`. Only used by the non-functional wallet subcommands and to build the default RPC URL path. |

The YAML config file has exactly one key:

```yaml
hidden:
  - 8d363b28528b0cb86b5fd48615493fb175bdf132d2a3d20b4251bba3f130a5abi0
```

`hidden` is a list of inscription ids whose content and preview the server will
not serve. It is a moderation control on one instance's HTTP surface. It does
not remove anything from the index, and another instance serving the same
database will serve the content normally.

`ord server` options
--------------------

These come after the `server` subcommand.

| Option | Default | Notes |
| --- | --- | --- |
| `--address <ADDRESS>` | `0.0.0.0` | **Binds to every interface by default.** Set `127.0.0.1` and put a reverse proxy in front. |
| `--http` | | Serve plain HTTP. |
| `--http-port <PORT>` | 80 | Implies `--http`. |
| `--https` | | Serve HTTPS with an ACME certificate. |
| `--https-port <PORT>` | 443 | Implies `--https`. |
| `--redirect-http-to-https` | | Requires both HTTP and HTTPS to be enabled. |
| `--acme-domain <DOMAIN>` | | Request a Let's Encrypt certificate for this domain. The instance must be reachable at `<domain>:443`. Repeatable. |
| `--acme-contact <CONTACT>` | | ACME contact, for example `mailto:ops@example.com`. Repeatable. |
| `--acme-cache <PATH>` | | Directory for ACME certificate state. |

A worked example
----------------

A full index (inscriptions, transactions, DRC-20 and Dunes) serving on
loopback:

```shell
export SUBSIDIES_PATH=/opt/ord-dogecoin/subsidies.json
export STARTING_SATS_PATH=/opt/ord-dogecoin/starting_sats.json
export RUST_LOG=info

ord \
  --rpc-url=http://127.0.0.1:22555 \
  --cookie-file=/etc/dogecoin/ord-rpc.cookie \
  --index=/data/ord-dogecoin/index-all.redb \
  --first-inscription-height=4609723 \
  --db-cache-size=8589934592 \
  --nr-parallel-requests=4 \
  --index-drc20 \
  --index-dunes \
  --index-transactions \
  server --http --address=127.0.0.1 --http-port=8391
```

An inscription-only index, which is much cheaper:

```shell
ord \
  --rpc-url=http://127.0.0.1:22555 \
  --cookie-file=/etc/dogecoin/ord-rpc.cookie \
  --data-dir=/data/ord-dogecoin \
  --first-inscription-height=4609723 \
  --nr-parallel-requests=16 \
  index
```
