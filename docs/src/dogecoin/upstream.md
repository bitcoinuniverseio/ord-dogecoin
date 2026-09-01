Upstream relationship and attribution
=====================================

This repository is not original work. It is the fourth link in a chain of
forks, and every earlier link deserves the credit.

Lineage
-------

| Project | Role |
| --- | --- |
| [casey/ord](https://github.com/casey/ord) | The original Ordinals indexer, wallet and explorer for Bitcoin. Ordinal theory, the inscription envelope, the redb index design, the explorer templates, and this handbook all originate here. |
| [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) | Ported `ord` to Dogecoin: the Dogecoin genesis block, the Dogecoin subsidy schedule, and inscription parsing from `scriptSig` instead of the Taproot witness. Also maintains the patched [rust-dogecoin](https://github.com/apezord/rust-dogecoin) and [rust-dogecoincore-rpc](https://github.com/apezord/rust-dogecoincore-rpc) crates this build depends on. |
| [verydogelabs/wonky-ord-dogecoin](https://github.com/verydogelabs/wonky-ord-dogecoin) | Added the real per-block Dogecoin rewards for heights 0 to 144,999, DRC-20 indexing, Dunes indexing, the transaction index, parallel RPC fetching, the address index, and the `openapi.yaml` contract. |
| [Trac-Systems/ord-dogecoin](https://github.com/Trac-Systems/ord-dogecoin) | The direct upstream of this repository. Moved to redb 2.4.0, which is not database-compatible with earlier `wonky-ord` builds. |
| **bitcoinuniverseio/ord-dogecoin** | This repository. See below. |

`Cargo.toml` still carries `homepage` and `repository` pointing at
`apezord/ord-dogecoin`, and the crate version is still upstream's `1.0.2`.
Those fields have deliberately not been rewritten: the package identity belongs
to the upstream release line, not to this fork.

License
-------

Everything in this lineage is published under
[Creative Commons Zero v1.0 Universal](https://github.com/bitcoinuniverseio/ord-dogecoin/blob/develop/LICENSE)
(CC0-1.0), a public domain dedication. That is the license of this repository
too, and the license any contribution to it is accepted under. The full text is
in `LICENSE` at the repository root.

CC0 waives copyright. It does not waive trademark rights and it offers no
warranty of any kind.

What this fork changes
----------------------

The authoritative short summary lives in `BITCOIN_UNIVERSE_FORK.md` at the
repository root. The changes so far:

**Build and toolchain**

- Requires Rust 1.88 or newer, matching the pinned dependency graph in
  `Cargo.lock`.
- CI compiles the production library and binaries on Linux and Windows,
  runs Clippy over the production targets, checks the forbidden-word list,
  builds the release binary and smoke-tests its `--version`, and builds this
  handbook with mdbook.

**Correctness**

- `--index-drc20` now controls the DRC-20 index independently of
  `--index-dunes`. In the affected upstream build the flag did not persist on
  its own. An index created by that build without DRC-20 data must be rebuilt;
  adding the flag to an existing database cannot backfill historical protocol
  state, because feature flags are recorded once at creation.
- `ord repair-address-index` repairs stale rows in the address index. See
  [Operations](operations.md#repairing-the-address-index).

**Machine-readable API surface**

A set of `/api/v1/*` endpoints was added for downstream services that need
exact, bounded, self-describing answers rather than HTML or best-effort JSON.
Their shared design rules, and why each one exists, are in
[HTTP API](http-api.md#the-v1-machine-contract). The most important of them is
`/api/v1/capabilities`, which exists because an index built without
`--index-drc20` answers every DRC-20 query with an empty list, and an empty
list is indistinguishable from a chain that genuinely has no tokens.

**Deployment**

`deploy/linux/` and `deploy/aws-doge-index/` hold systemd units and migration
scripts used to run and move a full index. They are examples of a working
configuration, not a supported product surface.

Release lines are not shared
----------------------------

This repository carries the tags `1.0`, `1.0.1` and `1.0.2`. **Those are
upstream's tags, inherited with the history.** They are not Bitcoin Universe
releases and they do not describe the state of this fork.

There is no Universe release of `ord-dogecoin`. `docs.manifest.json` therefore
declares `"lifecycle": "experimental"` with an `upstream` block, and declares
no `releasedRef` or `releaseVersion`. See
[Releases and versioning](releases.md).

Inherited code that is not maintained here
------------------------------------------

Some of what you will find in the tree came along with the fork and is not
exercised by this project:

| Path | Status |
| --- | --- |
| `src/subcommand/wallet/` | Gates on Bitcoin Taproot descriptors. Non-functional against Dogecoin Core. |
| `docs/src/guides/`, `docs/src/bounty/`, `docs/src/inscriptions/`, `bip.mediawiki` | Upstream Bitcoin ordinal theory material. Accurate for Bitcoin, not rewritten for Dogecoin. |
| `tests/` other than `compatibility.rs` and `authority_api_contract.rs` | Upstream fixture corpus. Not a Cargo test target. See [Testing](testing.md). |
| `.github/workflows/release.yaml` | Upstream tag-triggered binary release workflow. Not used by this fork. |
| `Vagrantfile`, `deploy/checkout`, `deploy/setup`, `justfile` deploy recipes | Upstream deployment tooling targeting `ordinals.net`. Not used by this fork. |
| `benchmark/`, `contrib/`, `quickstart/`, `examples/` | Upstream developer scratch material. |
