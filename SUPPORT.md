# Support

## Read this first

Most questions about this software have the same three answers, and all three
are in the documentation:

1. **"The DRC-20 endpoints return nothing."** Index feature flags are fixed
   when the database is created. Check `GET /api/v1/capabilities`. See
   [Troubleshooting](docs/src/dogecoin/troubleshooting.md).
2. **"It panics on startup."** `SUBSIDIES_PATH` and `STARTING_SATS_PATH` are
   mandatory. See
   [Configuration](docs/src/dogecoin/configuration.md#required-environment-variables).
3. **"The server is up but the data is stale."** The indexer runs inside the
   server process and can stop while HTTP keeps answering. See
   [Operations](docs/src/dogecoin/operations.md#what-to-alert-on).

The full index is in [`docs/src/dogecoin/`](docs/src/dogecoin), starting at
[Ord for Dogecoin](docs/src/dogecoin.md). The HTTP contract is
[`openapi.yaml`](openapi.yaml).

## Where to ask

| | |
| --- | --- |
| A bug, or behaviour that contradicts the documentation | [Open an issue](https://github.com/bitcoinuniverseio/ord-dogecoin/issues) |
| A security vulnerability | [SECURITY.md](SECURITY.md). Not a public issue. |
| A change you want to make | [CONTRIBUTING.md](CONTRIBUTING.md) |

## What to include in an issue

Without these, an issue about indexing usually cannot be acted on:

- The exact command line, including every index feature flag.
- The output of `GET /api/v1/capabilities`.
- The Dogecoin Core version, and whether `txindex=1` is set.
- Relevant log lines with `RUST_LOG=info`.
- Whether the database was built from scratch or restored from a snapshot, and
  which one.

Do not paste RPC credentials, cookie file contents, or private hostnames. Redact
them.

## What is not supported

- **`ord wallet ...`** Inherited upstream code that cannot work against
  Dogecoin Core. See
  [Differences from Bitcoin ord](docs/src/dogecoin/differences.md#the-wallet-subcommands-do-not-work-on-dogecoin).
- **Networks other than mainnet.** `testnet` and `regtest` carry Dogecoin
  values but are not exercised. `signet` is inherited from Bitcoin `ord` and
  should not be used.
- **The upstream pre-indexed database download.** It is a third-party artifact
  published outside this organization. Nothing here verifies it.
- **Third-party forks and older `wonky-ord` databases.** This build uses redb
  2.4.0 and index schema version 6. Databases from other builds are not
  compatible.
- **Recovering a corrupted index.** There is no repair tool for a database
  that was killed mid-write beyond redb's own recovery on reopen, and no
  migration between schema versions. Keep snapshots. See
  [Operations](docs/src/dogecoin/operations.md#backups).

## Response expectations

This repository is `experimental` in the Bitcoin Universe lifecycle and has no
release line and no support commitment. Issues are read; there is no service
level attached to them.
