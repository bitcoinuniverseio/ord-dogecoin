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
