Releases and versioning
=======================

There is no Universe release line
---------------------------------

This repository has **published no releases**. The tags it carries, `1.0`,
`1.0.1` and `1.0.2`, came with the fork and belong to upstream's release line.
`Cargo.toml` still says `version = "1.0.2"` for the same reason.

Do not read those tags as a statement about this fork. A commit on `develop`
that has been through CI is a truer description of what runs than any of them.

Accordingly, `docs.manifest.json` declares:

```json
"lifecycle": "experimental",
"upstream": {
  "project": "Trac-Systems/ord-dogecoin",
  "url": "https://github.com/Trac-Systems/ord-dogecoin",
  "license": "CC0-1.0",
  "relationship": "fork"
}
```

with no `releasedRef` and no `releaseVersion`, because there is nothing honest
to put in them. A repository becomes `stable` in this organization only with a
real Universe release tag.

Branches
--------

| Branch | Role |
| --- | --- |
| `develop` | Primary working branch and the default branch. Every change lands here first. |
| `main` | Production release branch. Promoted from `develop` after CI passes. |

The fork keeps upstream history and an `upstream` remote, so upstream changes
can still be merged.

What CI enforces
----------------

`.github/workflows/ci.yaml` runs on pushes and pull requests to `develop` and
`main`:

| Job | What it does |
| --- | --- |
| `lint` | Clippy over the production targets with `--all-features`, `rustfmt --check` on `tests/compatibility.rs`, `./bin/forbid`, and syntax and control checks on the AWS Dogecoin migration scripts. |
| `test-linux` | `cargo check --locked --workspace --lib --bins --all-features`, then both test targets. |
| `test-windows` | The same, on Windows. |
| `docs` | Builds this handbook with mdbook 0.4.52 and mdbook-linkcheck 0.7.7. |
| `release` | Builds `--locked --release --bin ord` and smoke-tests `ord --version`. |

Everything runs on the organization's self-hosted runner labels.

`.github/workflows/release.yaml` is upstream's tag-triggered workflow that
builds and publishes binaries for four targets. It is inherited and unused: no
tags are cut here.

Making a change
---------------

1. Branch from `develop`.
2. Make the change. Keep it surgical.
3. Run locally what CI runs:

   ```shell
   cargo clippy --locked -p ord-dogecoin --lib --bin ord \
     --test compatibility --test authority-api-contract --all-features
   cargo test --locked --test compatibility
   cargo test --locked --test authority-api-contract
   ./bin/forbid
   ```

4. If you touched an `/api/v1` payload, update `openapi.yaml` and
   `tests/authority_api_contract.rs` in the same commit.
5. If you touched the index schema, bump `SCHEMA_VERSION` in `src/index.rs` and
   say so loudly. A schema bump forces every operator to rebuild or restore.
6. Update `docs.manifest.json` `lastVerified` if you verified the manifest
   against reality.
7. Open a pull request into `develop`.

Compatibility rules
-------------------

Three kinds of change break operators, in descending order of pain:

**Schema changes.** Bumping `SCHEMA_VERSION` makes every existing database
unopenable. There is no migration path. This is a multi-day rebuild for anyone
running a full mainnet index. Do it only when there is no alternative, and
announce it.

**Index feature changes.** Feature flags are written at database creation and
cannot be added later. A change that makes a route depend on a new flag
silently breaks existing databases unless the route fails closed with a message
naming the flag, the way `DRC20_INDEX_ABSENT` does.

**API payload changes.** The `/api/v1` contract is consumed by services that
settle value. Adding a field is safe. Changing a type, a name, or the meaning
of a bound is not. Every quantity stays a string.

License
-------

CC0-1.0. See [Upstream relationship](upstream.md#license).
