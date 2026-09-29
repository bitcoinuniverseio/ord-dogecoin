# Dogemap dependency preparation: shared Doginals authority

Status: IMPLEMENTED on branch feat/dogemap-feed-20260929 (feed dogemap-feed-v1, P-02 journal, P-03 parser qualification). The text below is the preparation record and its FAIL/NOT TESTED rows describe the baseline before that work; coverage.json is kept as that baseline.

## Implementation record (2026-09-29)

- P-03: the compat parser (`doginals-trac-1.0.2-compat-v1`) is kept byte for byte, including the PUSHDATA2/PUSHDATA4 length quirk (P-F03), vin0-only reading and txid-keyed continuation (P-F04). Bounds hardening changes no outcome on 64-bit builds (an empty scriptSig list returns none instead of panicking; the PUSHDATA4 end offset is checked). A Core-correct decoder serves only `ord dogemap-pushdata-audit`. Vectors: tests/dogemap_parser_compatibility.rs.
- P-02: DOGEMAP_FEED_META (database id, creation coverage, journal range) and DOGEMAP_FEED_JOURNAL (per-block location records for prefilter-passing creations and every transfer) are written in the block's write transaction. Creations are derived from the inscription tables for any height instead of being stored. Transfer history before the journal start is reported `not-journaled`; no historical replay is performed.
- P-01: the four routes of contract section 2 read identity, checkpoint and data from one redb read transaction; node RPC facts (completion position for unjournaled blocks, first reveal block of multipart inscriptions, scripts for /locations) are fetched afterwards and re-checked against the index. Tests: tests/dogemap_feed_contract.rs, golden vector docs/contract/eventsHash-golden-v1.json.
- Every IMPLEMENTATION-HANDOFF [P-0x] marker was replaced by a rationale comment (annotation-index.json lists them as IMPLEMENTED).

This directory is the maintained implementation handoff for the Doginals provider dependency of dogemap-indexer and dogemap-renderer. It prepares the existing Universe ord-dogecoin authority to expose complete, reproducible Doginals creation and ownership data. It does not define Dogemap claim semantics.

## Exact baseline

| Item | Value |
| --- | --- |
| Repository | https://github.com/bitcoinuniverseio/ord-dogecoin |
| Fetched branch/base | origin/develop at ab2934f3ae027160274ff199fb7a19fd030041a8 |
| Isolated preparation branch | audit/dogemap-feed-prep-20260929 |
| SERVER worktree | `D:\universe\ord-dogecoin\.worktrees\dogemap-feed-prep-20260929` |
| Package / database schema | ord-dogecoin 1.0.2 / schema 6 |
| Declared Rust minimum | 1.88 |
| Existing installed toolchain | 1.96.0-x86_64-pc-windows-msvc; no default configured |
| Evidence directory outside repository | `D:\universe\ord-dogecoin\audits\dogemap-preparation-20260929` |
| Audit date | 2026-09-29 UTC |
| Prepared commit | Integration owner records final commit after annotation review |

The older shared checkout was not edited. The source baseline is not evidence that this revision is deployed. Runtime inventory belongs to the integration owner's separate evidence. Do not copy raw AGENTS.md, scripts containing credentials, index databases or wallet material into the handoff.

## Read in this order

1. The master implementation prompt and the dogemap-indexer I-01 protocol decision.
2. sources.md and findings.md: pinned evidence, confirmed defects and unresolved protocol questions.
3. feed-contract.md: the proposed shared P-01/I-02 contract.
4. work-packages.md: P-03 parser qualification, P-02 event history, then P-01 API.
5. annotation-index.json: actual source locations to edit during implementation; coverage.json: 14 independent functional requirements with exact status and evidence.
6. The exact prepared source files, then the parent handoff's runtime/coverage evidence.

The API, persistence and parser work is preparatory only. Every source marker is non-executable. New source/test paths named by a marker are PROPOSED NEW and have not been created.

## Dependency and operation map

| Existing component | Current role | Required local change | Consumer |
| --- | --- | --- | --- |
| src/inscription.rs | Parses scriptSig pushes and intrinsic body/delegate fields | P-03 qualified versioned parser and vectors | Shared inscription updater |
| src/index/updater/inscription_updater.rs | Tracks locations and transient operations | P-02 origin/completion and historical from/to events | Feed writer, existing DRC-20 |
| src/index/updater.rs | Indexes block and commits block hash | P-02 atomic immutable block manifest and coverage | Snapshot reader |
| src/index.rs / rtx.rs | Redb tables and individual read helpers | P-02 additive storage; P-01 one transaction per response | Feed HTTP handlers |
| src/index/reorg.rs | Savepoint rollback; durable Statistic::Reorgs | P-02 restore feed state and expose generation | P-01; indexer I-05 |
| src/authority_api.rs / subcommand/server.rs | Live inventory, capabilities, content/output endpoints | P-01 additive complete block/event/body API | dogemap-indexer I-02 |
| dogemap-indexer | Dogemap interpretation, ownership projection and read API | I-01/I-02/I-05 consume qualified feed | Renderer and core integration |

All production chain data must remain on Universe-owned infrastructure. Extend and reuse the shared authority. A one-time controlled historical projection replay is maintenance of this authority, not a separately deployed authoritative indexer. No parser, configuration, dependency, interface, database or service was changed by preparation.

## Coverage matrix

| ID | Required outcome | Baseline status | Cause / package | Required evidence |
| --- | --- | --- | --- | --- |
| P-C01 | Network/genesis/profile/schema/checkpoint identify one snapshot | FAIL, source-confirmed missing contract | P-F01/P-F05; P-01 | Atomic capability/page identity and network mismatch tests |
| P-C02 | Every covered block has a complete ordered creation/transfer set, including zero events | FAIL, required capability absent | P-F01/P-F02; P-02 then P-01 | Manifest cardinality/digest, pagination/replay/crash tests |
| P-C03 | Intrinsic original bytes/type/delegate provenance are retrievable | FAIL, feed contract absent | P-F01/P-F02; P-02/P-01 | Full-byte/digest proof and unavailable-body behavior |
| P-C04 | Historical ownership preserves script, output value and offset | FAIL, retained history absent | P-F02/P-F06; P-02 | Multi-input/output, same-block, non-address and fee/lost vectors |
| P-C05 | Recoverable forks and restart invalidate stale data/cursors | FAIL for feed; existing rollback mechanism inspected only | P-F05/P-F02; P-02/P-01 | Reorg during paging, restore, epoch, rewind and replay tests |
| P-C06 | Published parser/order profile agrees with governing Doginals/Dogemap evidence | BLOCKED on I-01 profile evidence; decoder defect FAIL | P-F03/P-F04; P-03 | Independent raw transaction corpus and historical impact comparison |
| P-C07 | Existing shared authority consumers retain their contracts and valid behavior | NOT TESTED | Every P package | authority-api-contract, inscription-json, drc20-decisions and changed dependencies |

P-C01..P-C05 failures identify required new integration capabilities missing in source; they are not claims of observed production outages. Source review is not functional PASS. P-C06's consensus/profile ambiguity must remain explicit.

## Preparation verification and handoff evidence

The integration owner receives command logs and source snapshots in the evidence directory. checks/ contains comment-only assurance, Git whitespace checking and Rust parser results when run. The preparation verifier must remove only the exact inserted comment bytes and recover each baseline file byte-for-byte. It must also show no removed source lines, no changed Cargo/config/dependency files, all marker/index links resolving, and clean Git whitespace.

Future automated commands in work-packages.md are not reported as executed. Public Dogecoin Testnet is the functional blockchain gate because pinned Dogecoin Core supports mainnet/testnet/regtest and has no Signet chain. Local deterministic reorg/crash fixtures supplement Testnet; they do not replace it. Mainnet release occurs only in the later implementation phase after the parent acceptance gates.

## Operational scope and rollback

Provider endpoints are a dependency, not an independent product expansion. Do not add trading features or replace existing DRC-20/Dunes ledgers. Parser changes may affect those consumers and therefore need their targeted regression evidence. A prepared comment never authorizes an unreviewed data reinterpretation.

Keep the serving shared process healthy. Do not restart or stop an authority that is processing a valid reorg or indexing. Additive feed storage must start unavailable and become ready only after retained coverage and required replay are proved. Keep the previously accepted binary/configuration and data restoration plan; invalidate feed consumers on generation changes and avoid rolling an old binary onto incompatible state.

## Executed preparation assurance

Executed 2026-09-29 on SERVER with Rust toolchain 1.96.0 and rustfmt 1.9.0-stable. Exact removal of the 12 added comment blocks restores all eight source files byte-for-byte. Cargo.toml and Cargo.lock are unchanged. Every annotation-index entry resolves. git diff --check passed. Each modified Rust file parsed successfully with rustfmt --edition 2021 --emit stdout --config skip_children=true; formatted stdout was discarded, so no formatting was applied. These checks establish preparation integrity only. No compile/build, functional test, migration, reindex, service restart or deployment was performed.

Evidence: external audit checks/comment-only.json and checks/preparation-checks.json, with the verifier and per-file stderr logs. Initial bare Rust wrapper calls reported that no default toolchain was configured; explicitly selecting the already-installed 1.96.0 toolchain resolved parser checking without installation or environment mutation.
