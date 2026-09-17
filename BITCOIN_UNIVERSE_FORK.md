# Bitcoin Universe fork provenance

This repository is the Bitcoin Universe production fork of
`Trac-Systems/ord-dogecoin`.

- Upstream baseline: `1ae4f3fe832493042612034130af2ea68d9f39ac`
- Upstream release: `1.0.2`
- Production integration branch: `develop`
- Production release branch: `main`

The fork keeps upstream history and an `upstream` remote. Bitcoin Universe
changes must pass the pinned Linux and Windows CI before promotion to `main`.

CI compiles the production library and binaries on Linux and Windows,
runs Clippy's enforced correctness lints, tests the exact authoritative authority
JSON contract, builds the release binary, smoke-tests its CLI, and builds the
documentation. The inherited inline upstream test modules are not a release
gate because this source snapshot combines incompatible Bitcoin API, redb,
and Ord test fixtures and does not compile them as a single `cfg(test)` crate.
Production compatibility is instead covered by the cross-platform build,
focused authority contract tests, and deployment smoke tests against the
isolated Dogecoin Core 1.14.9 txindex node. The incompatible upstream tests
remain in-tree for incremental repair and are not represented as passing.

The fork requires Rust 1.88 or newer, matching the minimum supported version
of the patched dependency graph recorded in `Cargo.lock`.

The first production patch corrects new-index feature persistence so
`--index-drc20` controls the DRC20 index independently of `--index-dunes`.
Indexes created by the affected upstream build without DRC20 data must be
rebuilt; the flag cannot add missing historical protocol state to an existing
database.

The DRC-20 index retains a per-operation decision. Every deploy, mint,
inscribe-transfer and transfer the ledger evaluates is recorded as accepted
or rejected with its protocol reason, in the same redb write transaction as
the ledger change, in `DRC20_OPERATION_DECISIONS`. Savepoint rollback on a
reorg removes verdicts together with the balances they explain.
`GET /api/v1/drc20/operations/{inscriptionId}` and
`GET /api/v1/drc20/operations?txid=` serve them with the deciding block,
the rule set (`drc20-v1`) and the index's reorg epoch, and answer
`not-evaluated` with a reason wherever no verdict exists. An existing
database needs no rebuild: it records verdicts from the next block it indexes
and reports that height as `drc20DecisionsFromHeight` on
`/api/v1/capabilities`, which also reports the configured `network`.
Operations below that height stay `outside-decision-coverage`; they are
never inferred from an inscription's `drc-20` marker or from today's
balances. See `docs/src/dogecoin/http-api.md`.
