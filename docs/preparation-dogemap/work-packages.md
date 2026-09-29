# Dependency-ordered provider work packages

Preparation scope: actual non-executable source comments and an execution-ready dependency handoff. The next implementation agent must use the parent prompt and exact prepared commit, read annotation-index.json, and work in dependency order. Check targeted baseline drift; a new broad audit is not required.

Order: I-01 protocol/profile decision -> P-03 parser qualification -> P-02 retained events/coverage -> P-01 feed API -> dogemap-indexer I-02/I-05 -> downstream I-06/I-08 and renderer/core integration. Resolve new evidence gaps locally; never convert ambiguity into a guessed protocol rule.

## P-03: Qualified Doginals parser and event order

Coverage P-C06/P-C07. Findings P-F03/P-F04. Prerequisite I-01 rules/profile decision; supplied research already records the open questions. Source markers PARSER-PROFILE, PARSER-PUSHDATA and PARSER-CONTINUATION.

Affected files: src/inscription.rs; src/index/updater/inscription_updater.rs; I-01 protocol/profile artifacts in dogemap-indexer. PROPOSED NEW test file tests/dogemap_parser_compatibility.rs, explicit Cargo target dogemap-parser-compatibility. If the parser is extracted into a versioned module, mark that exact new path in the implementation diff and wire the existing source owner; no parallel authority.

1. Pin profile ID, parser commit, accepted encoding and order rules from I-01. Consume the supplied external protocol-source-manifest and raw source evidence; distinguish normative Dogemap decisions from reference parser behavior.
2. Build independent byte/transaction vectors for direct/PUSHDATA1/2/4 pushes, length truncation, split reveal completion, sibling/nonzero vout, vin1, old inscriptions at offset zero, fees/coinbase and partial-state reorg. Keep expected decisions separate from the provider algorithm.
3. Repair PUSHDATA2/PUSHDATA4 length extraction using the following two/four LE bytes with checked arithmetic and bounds. Preserve opcode acceptance rules unless evidence requires a separately documented profile change.
4. Resolve continuation keys, input selection, inscription index, first reveal ID, origin/completion time and competing-claim order from the elected profile. Capture both origin and completion coordinates. Do not assume entry height/number/sequence is the correct priority.
5. Name every changed parse outcome and affected height/candidate class in a reproducible comparison report. If the qualified profile changes historical state, stage a versioned projection replay and downstream reconciliation; never let an old database claim the new profile merely by changing a label.
6. Run production parser/indexing paths through the new explicit integration target. Cargo.toml has lib test=false, so private unit tests alone do not prove execution. Keep existing wallet/inscription, DRC-20 and Dunes consumers compatible or repair/retest those actually affected by the shared parser changes.

Acceptance: all profile decisions that can alter Dogemap validity/first-claim/ownership have authoritative evidence and independent expected results; raw intrinsic bytes and ID/order/coordinates match across provider and consumer; malformed scripts fail deterministically without panic, overflow or unbounded allocation. BLOCKED remains appropriate where semantics are unresolved.

Exact future commands after implementing/registering tests (not executed in preparation): cargo +1.96.0 test --locked --test dogemap-parser-compatibility; cargo +1.96.0 test --locked --test inscription-json --test authority-api-contract --test drc20-decisions --test compatibility. These are command forms pinned to the installed toolchain, not evidence that build prerequisites or tests currently pass.

Rollback: retain prior accepted parser/profile and compatible data state. Do not run the old interpretation against state mutated by the new one. Restore or select the prior qualified projection, rotate generation, invalidate consumers and replay only verified data. Changes to shared protocols require their dependency regression evidence.

## P-02: Immutable block events, historical ownership and rollback-aware coverage

Coverage P-C02/P-C03/P-C04/P-C05/P-C07. Findings P-F02/P-F05/P-F06. Prerequisite P-03, including I-01 decisions. Source markers FEED-STORAGE, FEED-EVENTS, FEED-COMMIT and FEED-REORG.

Affected files: src/index.rs; src/index/updater.rs; src/index/updater/inscription_updater.rs; src/index/reorg.rs. PROPOSED NEW src/index/dogemap_feed.rs for typed durable structures/read-write helpers and tests/dogemap_feed_contract.rs for integration. Wire the module into the existing index and register Cargo target dogemap-feed-contract during implementation.

1. Add feed-specific versioned metadata and additive tables to schema 6: database generation/qualified profile; block manifests keyed by profile/height/hash; ordered event rows; immutable intrinsic body objects; retained coverage segments and contiguous feed watermark. Keep existing tables and readers compatible; prove this using an old binary read/rollback check before electing an additive migration. Incompatible changes require explicit migration and protected restoration, not deletion instructions.
2. Extend the shared InscriptionUpdater context with original block transaction coordinates and a feed event recorder. Capture previous output raw script/value/offset before it is removed and exact new output state before mutable lookups can move again. Preserve intrinsic bytes/type/delegate, first reveal and completion coordinates on creations.
3. Include all inscription-enabled blocks and all movement types independently of DRC-20/Dunes flags. Reuse existing tracking and fee allocation. Capture transient operations before consumers take them; leave DRC-20's own decisions and ordering unchanged unless P-03 evidence requires a tested repair.
4. Finalize fee movements to coinbase/lost state and use the elected orderProfile to assign dense ordinals. Validate outpoint/offset/value invariants, coordinates and body digests. Do not silently coerce missing scripts to an address or skipped events.
5. Within the same existing redb write transaction, write bodies/events/manifest with exact event count/hash, block parent/hash, coverage and watermark together with HEIGHT_TO_BLOCK_HASH. A zero-event block gets a complete manifest too. Missing data or integrity errors abort publication. Interrupted or duplicated work is idempotent by identity; a conflicting digest is an explicit fault.
6. On upgrade, initialize feed coverage as unavailable. Retain forward events from a recorded height. Use a bounded one-time history projection replay through the same qualified provider code against Universe's archival Dogecoin node to fill required earlier coverage. Start at the proven protocol history boundary or a verified reconstructible checkpoint; never choose a convenient recent start that loses earlier claims.
7. The replay is maintenance of the shared authority and may stage projection output on approved indexing infrastructure, not a new public Doginals service or a duplicate live indexer. Use single-writer coordination and resource limits. Keep serving the existing authority. Resume using height/hash/profile checkpoints; verify overlaps with live event manifests and compare derived current locations before merging coverage segments.
8. Reuse Statistic::Reorgs: handle_reorg already retains its prior value across savepoint restoration and increments it atomically. Restore feed tables/watermark alongside ledger state. Expose the new epoch through P-01 and invalidate old cursors. External restores/rebuilds rotate databaseId before readiness; retained epoch numbers alone are insufficient.
9. For deep/unrecoverable forks, expose unavailable state and halt consumer advancement. Do not clear the error or synthesize missing history. Use the accepted restore/replay process with valid backups and adequate capacity.

Acceptance: every covered block is complete and immutable under the chosen profile; pages reproduce the retained block manifest; replay is gap-free where required; duplicates/restart/crash/reorg cause neither lost events nor stale owner/negative decisions. Historical current-state reconciliation passes at a shared height/hash. Existing shared consumers remain usable.

Future tests/commands: cargo +1.96.0 test --locked --test dogemap-feed-contract; then the existing target regression commands listed under P-03. The new test harness must launch the production ord binary against deterministic local test RPC/blocks and inspect real redb and HTTP effects. Use the current tests/drc20_decisions.rs harness as integration structure, while independently defining feed assertions. Cover:
- Existing DB without feed tables, first block, zero events, gap and range joining.
- Create+multiple same-block transfers, multiple inscriptions/output, nonzero offsets, non-address scripts, fee/lost state.
- DRC-20 off and on with identical Doginals feed, plus unchanged DRC-20 behavior.
- Crash before/after write commit, duplicate/reordered delivery and conflicting manifests.
- Reorg before/after multipart completion, while paging, after restart, and after restored checkpoint; consumer I-05 rewinds all dependent state.

Rollback: ship additive feed reading/writing disabled until qualified coverage exists. Use compatible prior binary/configuration for unaffected serving APIs. Stop feed readiness, not the shared live authority, when validation fails. Keep manifests and bodies versioned and restorable. Never downgrade onto incompatible migrated state or destroy a healthy shared index to obtain a clean test.

## P-01: Atomic complete-block and intrinsic-body HTTP feed

Coverage P-C01/P-C02/P-C03/P-C04/P-C05/P-C07. Findings P-F01/P-F05. Prerequisite P-02. Source markers FEED-WIRE, FEED-HTTP, FEED-CAPABILITIES, FEED-READ and FEED-SNAPSHOT.

Affected files: src/authority_api.rs; src/subcommand/server.rs; src/index.rs; src/index/rtx.rs. Implement feed-contract.md exactly and coordinate its test vector/schema version with dogemap-indexer I-02.

1. Add separate feed DTOs, strict parameter parsers and typed errors. All chain integers are decimal strings; scripts are raw hex, intrinsic bytes/type are encoded losslessly, address is nullable, and location unresolved/lost are distinct.
2. Implement one Index::dogemap_feed_snapshot entry point backed by one Rtx per response. Read identity, coverage/watermark, requested manifest, events/body descriptors and durable epoch from that transaction. Do not call live helper getters which begin additional reads or make unbound node calls mid-response.
3. Wire the three additive /api/v1/dogemap-feed routes. Preserve every existing output/inscription/content/capabilities contract. Read-only feed access follows existing service authentication/exposure boundaries and must not expose private node RPC or credentials.
4. Validate explicit network/genesis/profile/database identity, request height/hash/epoch and cursor. Enforce bounds before allocations. Return typed stale or unavailable responses and maintain observability. Do not turn exceptions, filtered content, missing manifests or unqualified parser cases into empty-success decisions.
5. Serve deterministic pages with dense ordinals, count/hash and terminal completeness. Include complete zero-event proof. Use exactly the descriptor projection in feed-contract.md and a shared golden RFC 8785 hash vector with the TypeScript consumer.
6. Inline bounded intrinsic bodies and serve larger content through the same-origin bounded chunk route. Bind every body read to generation and creation block hash, verify exact digest/length, and prevent arbitrary host redirects or delegate substitution.
7. Advertise actual feed coverage/profile, parser build, database generation, current epoch and limits from the same snapshot. Distinguish ordinary index tip from the feed watermark. readiness must fall on gaps, recovery or incompatible profile; cache keys include full identity and stale responses cannot survive a rollback.
8. Have I-02 assemble/validate a full block before any Dogemap decision and I-05 commit decisions/checkpoint atomically. Retry under bounded backoff; stale identity requires fork recheck, not unconditional progress.

Acceptance: response data and identity never mix read roots; every complete block can be reproduced by digest; missing/lagged/changed data yields a truthful error; no incorrect negative claims; exact integers and scripts survive Rust->JSON->TypeScript. The same event semantics hold at any page limit and inline-body threshold.

Future command: cargo +1.96.0 test --locked --test dogemap-feed-contract, followed by cargo +1.96.0 test --locked --test authority-api-contract --test inscription-json --test drc20-decisions. Concurrent commit/paging, epoch reuse, old cursor, malformed bounds, zero block, intrinsic/delegate, chunk truncation, >2^53 values and cache invalidation assertions are required. Pair with I-02/I-05 consumer tests and actual end-to-end Testnet retrieval/ownership/rendering. These new tests and outcomes remain NOT TESTED here.

Rollback: revoke feed readiness/exposure and roll consumers to the last accepted height/profile/generation without changing unrelated authority routes. Restore a compatible binary/data state and invalidate downstream caches. Retain evidence to replay accepted blocks; never serve an old snapshot as current.

## Shared functional and public-release gate

Use the actual configured Universe Dogecoin Testnet authority/node and isolated funded test wallets for required transaction journeys. Dogecoin Core's pinned chain definitions have no Signet; testnet is the justified public verification network. Use deterministic local fixtures for faults unsafe to create publicly. An offline or serialization-only test is not an end-to-end PASS.

Required real journey: Doginals creation on Testnet -> qualified provider completion/event/body -> full-block ingestion -> Dogemap interpretation/ownership -> authoritative API readback -> renderer outcome -> refresh/reconnect/restart persistence. Transfer lifecycle and every affected negative, duplicate, retry and recovery boundary are verified. All rows must retain revision, network, time, source/transaction identity, assertion and evidence.

Mainnet configuration verification must independently check genesis/network, parser/activation/profile, history coverage, shared RPC/provider identity, build provenance, storage capacity, credentials through the existing secure mechanism and rollback. Mainnet transactions/funds are not required for acceptance. Do not mutate production defaults for a test pass.

After the parent's full functional gate, release through the existing authorized production process without downtime, deploy the accepted revisions, enable the intended public Dogemap routes/capabilities, and record receipts, versions, service health and actual public URLs. The current preparation performs no deployment or release. Runtime topology and final integration/PR ownership belong to the root handoff.
