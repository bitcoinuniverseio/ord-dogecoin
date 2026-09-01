# Contributing

Thanks for wanting to help. This is a fork of upstream `ord` for Dogecoin, so
the most useful contributions are usually small, verifiable, and specific to
what Dogecoin does differently.

## License

Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in this work is licensed as in [LICENSE](LICENSE)
(CC0-1.0), without any additional terms or conditions.

## Before you start

- Read [Differences from Bitcoin ord](docs/src/dogecoin/differences.md). Most
  surprising behaviour in this codebase is a deliberate Dogecoin difference,
  not a bug.
- Read [Upstream relationship](docs/src/dogecoin/upstream.md) to see what is
  maintained here and what is inherited dead code.
- Check whether the change belongs upstream instead. A fix to ordinal theory
  itself probably belongs in `casey/ord`; a fix to Dogecoin address handling
  probably belongs in `apezord/rust-dogecoin`.

## Branches

`develop` is the primary working branch and the default branch. Branch from it,
and open pull requests into it. `main` is the production release branch,
promoted from `develop` after CI passes.

## What to run before opening a pull request

```shell
cargo clippy --locked -p ord-dogecoin --lib --bin ord \
  --test compatibility --test authority-api-contract --all-features
cargo test --locked --test compatibility
cargo test --locked --test authority-api-contract
rustfmt --check tests/compatibility.rs
./bin/forbid
```

That is what CI runs, on Linux and Windows. CI additionally builds the release
binary, smoke-tests `ord --version`, and builds `docs/` with mdbook.

`cargo test --all` does not work on this snapshot. Only
`tests/compatibility.rs` and `tests/authority_api_contract.rs` are Cargo test
targets; see [Testing](docs/src/dogecoin/testing.md).

## Rules that are not negotiable

**Schema changes.** Bumping `SCHEMA_VERSION` in `src/index.rs` makes every
existing database unopenable, with no migration path. For anyone running a full
mainnet index that is a multi-day rebuild. Do it only when there is no
alternative, and say so prominently in the pull request.

**Index feature flags.** Feature flags are written into the database at
creation and cannot be added later. If a change makes a route depend on a new
flag, the route must fail closed with a message that names the flag and says a
rebuild is required, the way `DRC20_INDEX_ABSENT` does. Returning an empty
result is not acceptable: downstream cannot tell it apart from a chain with no
data.

**API contract changes.** The `/api/v1` payloads are consumed by services that
settle value. Adding a field is safe. Changing a type, a name, or the meaning
of a bound is not. Every quantity stays an exact decimal string, never a JSON
number. Any change here needs a matching assertion in
`tests/authority_api_contract.rs` and a matching update to `openapi.yaml`, in
the same commit.

**Route changes.** `openapi.yaml` is declared as this repository's interface
contract in `docs.manifest.json`, and the portal generates its interface
directory from it. A route added without a matching path entry is a route
nobody can find; a path entry without a route is a documented endpoint that
returns 404. Keep them in step.

**The subsidy files.** `subsidies.json` and `starting_sats.json` define ordinal
numbering. Do not edit them without an extremely good reason and a plan for
every existing index.

## Style

- Follow the surrounding code. The tree is two-space indented Rust.
- Keep changes surgical. Do not reformat or refactor adjacent code.
- No em dash characters anywhere in code, comments, documentation or commit
  messages.
- `./bin/forbid` enforces a small list of words the project will not ship.

## Documentation

Documentation lives in `docs/src/dogecoin/` and is wired into
`docs/src/SUMMARY.md`. The rest of `docs/src/` is the upstream Ordinal Theory
Handbook, which describes Bitcoin; do not rewrite it as though it described
Dogecoin.

Every claim in the documentation should be traceable to code, to a file in the
repository, or to the published Universe capability snapshot. If you cannot
point at the source, do not write it down.

## Reporting bugs

Open an issue with:

- the exact command line, including which index feature flags the database was
  created with;
- the output of `GET /api/v1/capabilities`;
- the Dogecoin Core version;
- the relevant log lines at `RUST_LOG=info`.

For security vulnerabilities, do not open an issue. See
[SECURITY.md](SECURITY.md).
