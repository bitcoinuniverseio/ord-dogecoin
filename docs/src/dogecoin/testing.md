Testing
=======

What is actually a test target
------------------------------

`Cargo.toml` sets `autotests = false` and declares exactly three integration
test targets:

```toml
[[test]]
name = "compatibility"
path = "tests/compatibility.rs"

[[test]]
name = "authority-api-contract"
path = "tests/authority_api_contract.rs"

[[test]]
name = "drc20-decisions"
path = "tests/drc20_decisions.rs"
```

The `[lib]` section sets `test = false`, so the inline `#[cfg(test)]` modules
scattered through `src/` are **not compiled or run** either.

Everything else under `tests/` is inherited upstream fixture code that is not
built. Be precise about this: 29 files under `tests/` looks like broad
coverage, and three of them are the coverage.

Running the suite
-----------------

```shell
cargo test --locked --test compatibility
cargo test --locked --test authority-api-contract
cargo test --locked --test drc20-decisions
```

All three are what CI runs on Linux. The last one drives the built `ord`
binary against the in-process regtest node from `test-bitcoincore-rpc`, so
it needs `SUBSIDIES_PATH` and `STARTING_SATS_PATH` to resolve; the test
points them at the repository's `subsidies.json` and `starting_sats.json`
itself. The full lint pass:

```shell
cargo clippy --locked -p ord-dogecoin --lib --bin ord \
  --test compatibility --test authority-api-contract --test drc20-decisions \n  --all-features
rustfmt --check tests/compatibility.rs
./bin/forbid
```

`./bin/forbid` greps the tree for words the project refuses to ship.

`cargo test --all`, from the inherited `justfile` `ci` recipe, does **not**
work on this snapshot. The upstream fixture corpus mixes Bitcoin-era APIs, a
different redb version, and Bitcoin reward assumptions.

What `tests/compatibility.rs` covers
------------------------------------

Five tests, exercising the parts of the Dogecoin fork that break silently when
a pinned dependency moves:

| Test | What it protects |
| --- | --- |
| `dune_identifiers_round_trip_through_the_public_api` | `Dune` and `DuneId` string round-trips through the public API. |
| `dunestone_edicts_round_trip_with_the_pinned_dogecoin_script_api` | Dunestone script encoding and decoding against the pinned `rust-dogecoin` script API. |
| `non_dune_op_return_is_not_interpreted_as_a_dunestone` | An unrelated `OP_RETURN` is not misread as a dunestone. |
| `configured_redb_cache_applies_before_existing_index_open` | `--db-cache-size` is applied when opening an existing index, not only when creating one. |
| `repair_address_index_removes_spent_rows_and_backfills_live_rows` | `ord repair-address-index` removes stale rows and backfills live ones. |

What `tests/authority_api_contract.rs` covers
---------------------------------------------

Fifteen tests over the `/api/v1` JSON contract. They exist because these
payloads are consumed by services that settle value, so a shape change is a
production incident, not a cosmetic one.

They assert, among other things:

- Every `u128` quantity serializes as an exact decimal **string**, for DRC-20,
  for custody values and for dune supplies, so nothing is rounded through a
  JSON number.
- Limits and offset cursors are bounded and reject anything that is not a
  decimal position.
- The DRC-20 token catalog is independent of the transferable inventory, so a
  deployment with no outstanding transferable is not dropped.
- DRC-20 and dune payloads state whether the index can answer at all, so an
  empty result is never ambiguous.
- Index capabilities are reported.
- Funding proofs serialize exact cardinal values.
- Inscription inventory carries subsidy provenance.
- A dune definition reports its symbol without inventing one.

What `tests/drc20_decisions.rs` covers
--------------------------------------

Six end-to-end tests. Each one mines regtest blocks on the in-process test
node, carries DRC-20 operations in real script_sig inscriptions, indexes
them with the production binary (`ord index` or `ord server` with
`--index-drc20 --index-transactions`) and reads the verdicts back over
`/api/v1/drc20/operations`. They prove the ledger, the retained decision
and the HTTP contract together.

| Test | What it protects |
| --- | --- |
| `ledger_verdicts_are_retained_per_operation_with_their_block` | Valid deploy accepted; mint over the limit, mint of an undeployed ticker, duplicate deploy and inscribe-transfer beyond balance each rejected with the protocol reason; a plain inscription inside coverage is `not-a-drc20-operation`; an unknown inscription is 404; the ledger agrees with the verdicts. |
| `every_operation_in_one_transaction_is_a_separate_decision` | One transaction spending two inscribe-transfer inscriptions yields two transfer records under `?txid=`, without overwriting the inscribe-transfer verdicts. |
| `history_before_decision_coverage_is_not_evaluated_rather_than_inferred` | A database indexed without the table gains it on the next block without a rebuild, reports `drc20DecisionsFromHeight`, and answers older operations with `outside-decision-coverage`. |
| `reorg_rolls_decisions_back_with_the_ledger_and_reindexing_restores_them` | Invalidating the tip removes the orphaned block's verdict and ledger effect; re-mining the transaction re-derives both under an incremented `reorgEpoch`. |
| `capabilities_advertise_retained_decisions_and_their_coverage_start` | `/api/v1/capabilities` reports `network`, `drc20Decisions` and `drc20DecisionsFromHeight`. |
| `a_database_without_the_drc20_index_reports_decisions_as_disabled` | Without `--index-drc20` every verdict is `not-evaluated` with `drc20-index-disabled` and the collection route is 400. |

The server's index thread polls every five seconds, so each test spends most
of its wall-clock time waiting for a sync rather than computing.

What is not covered
-------------------

Stated plainly, because the gap matters:

- **End-to-end indexing is covered only through the DRC-20 decision suite.**
  It exercises a regtest chain, one savepoint rollback and the routes it
  needs; sat indexing, dunes and the remaining `/api/v1` routes are still
  proved by serialization tests alone.
- **No Dunes protocol-semantics tests.** Etching, minting and edict rules are
  exercised only in production.
- **No wallet tests**, which is consistent: the wallet subcommands do not work
  on Dogecoin.

Production compatibility is instead covered by the cross-platform build, these
three focused suites, and deployment smoke tests against an isolated Dogecoin
Core 1.14.9 node with `txindex`.

Fuzzing
-------

`fuzz/` is a separate `cargo-fuzz` workspace with four libFuzzer targets:

| Target | Input |
| --- | --- |
| `dunestone-decipher` | Arbitrary transactions, deciphered as dunestones. |
| `transaction-builder` | The transaction builder. |
| `varint-encode` | Dunes varint encoding. |
| `varint-decode` | Dunes varint decoding. |

These are the parsers that consume attacker-controlled bytes, which is the
right place to fuzz.

```shell
cargo install cargo-fuzz
cargo fuzz run varint-decode
```

The fuzz workspace carries its own `Cargo.lock` and its own
`[patch.crates-io]` section, and it is **not run in CI**. There is no corpus in
the repository and no regression harness. Treat it as a tool available to you,
not as coverage you already have.

Adding coverage
---------------

Add new tests to `tests/compatibility.rs` or `tests/authority_api_contract.rs`,
or add a new `[[test]]` target and wire it into `.github/workflows/ci.yaml`.
Migrating an upstream fixture is welcome; leaving it in place and unbuilt is
not a test.

A change to any `/api/v1` payload shape needs a matching assertion in
`tests/authority_api_contract.rs` and a matching update to `openapi.yaml`.
