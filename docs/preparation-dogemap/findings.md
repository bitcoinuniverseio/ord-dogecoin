# Provider findings and reproducible source evidence

Baseline for every provider finding: bitcoinuniverseio/ord-dogecoin ab2934f3ae027160274ff199fb7a19fd030041a8, inspected 2026-09-29. The older shared checkout and live runtime were not treated as this revision. Source snapshots are in the external evidence directory's baseline/ tree. No production requests or chain mutations were performed by this workstream.

## P-F01: No complete historical block feed

Status FAIL: required new integration capability absent. Affects P-C01/P-C02/P-C03, dogemap-indexer I-02/I-05 and renderer data freshness.

Reproduce by reading Server::inscription_inventory, Index::get_latest_inscriptions_with_prev_and_next and Rtx. The inventory obtains block_count, then block_hash, latest inscription IDs and each body/entry/current location through separate reads. It returns inventory_complete=true but does not bind a block, profile, generation or epoch. Its cursor is an inscription number in a newest-first global list. Index getters each begin their own redb read transaction. During a concurrent writer or reorg these calls can observe different committed roots. This is a static race/contract analysis, not an observed live corrupt response.

The inventory resolves delegate metadata to align with /content; an intrinsic protocol parser cannot use that representation as original body/type provenance. OutputDetail.script_pubkey is assembly; InventoryLocation.script_pubkey is hex, but only describes current state.

Remedy: P-02 retains block events/manifests, P-01 reads them atomically and adds the exact feed-contract.md routes without breaking the existing inventory. Reproduce the race with deterministic concurrent writer checkpoints, then assert one consistent response or typed unavailability. A zero-event block requires persisted completeness, not an empty page.

## P-F02: Required creation/transfer history is transient or overwritten

Status FAIL. Affects P-C02/P-C03/P-C04/P-C05 and consumer I-05 historical ownership/reorg recovery.

Reproduce in InscriptionUpdater::update_inscription_location: old current satpoint is removed, id_to_satpoint is overwritten and an InscriptionOp is pushed into an in-memory operations HashMap. Updater::index_block clones those operations only inside the DRC-20-enabled branch and passes them to Drc20Updater. There is no full Doginals creation/transfer journal or complete-block manifest among schema 6 table declarations. DRC-20 decisions are a separate, protocol-specific subset and cannot substitute for all Doginals events.

Remedy: capture input and output script/value/offset before removal; persist complete events, intrinsic bodies, ordering and block manifests in the same write transaction as block hash and coverage. Extend the same authority independently of DRC-20/Dunes flags. Test crash atomicity, repeat blocks, no-event blocks, transfers in one block, fee/lost movements and history reconciliation. Historical backfill cannot be inferred from current ownership.

## P-F03: PUSHDATA2 and PUSHDATA4 decode the wrong length bytes

Status FAIL, source-confirmed implementation defect. The effect on the set of Dogemap claims allowed by I-01 is not yet established; do not invent a protocol activation or silent historical correction.

Reproduce in InscriptionParser::decode_push_datas. For opcode 77, the length expression uses bytes[1] shifted left 8 plus bytes[0], omitting bytes[2]. For opcode 78 it uses bytes[3], bytes[2], bytes[1], bytes[0], omitting bytes[4]. Dogecoin Core's GetOp2 at P-S02 consumes the opcode first, reads the next two/four bytes with ReadLE16/ReadLE32, then checks remaining payload length. The transaction builder writes the matching LE format.

Minimal PUSHDATA2 vector: hex prefix 4d0001 followed by 256 bytes of aa. Core length is 256; this provider computes 77, consumes 77 payload bytes, then rejects the next aa opcode. PUSHDATA4 prefix 4e00010000 with 256 aa bytes computes 65614 here and fails bounds instead of consuming 256. These calculations describe the actual inspected expressions; the external preparation harness, when run, is separate from production integration acceptance.

Remedy: P-03 corrects length extraction and bounds under an explicit qualified parser profile, runs raw parser/transaction vectors, quantifies historical result changes, and coordinates replay/migration and regression of every affected shared consumer. Never relabel an existing database as the corrected profile without the appropriate replay.

## P-F04: Parser, continuation and competing-claim order need an explicit evidence decision

Status BLOCKED for normative Dogemap compatibility; current behavior verified. Affects P-C06 and I-01.

Current provider behavior:
- Inscription::from_transactions reads only input zero's scriptSig.
- A new parse is attempted only when no old tracked inscription has cumulative input offset zero.
- Partial assembly is keyed by previous txid, excluding vout.
- The inscription ID uses the first stored reveal txid with inscription index zero.
- Entry height/timestamp are set where assembly completes; original reveal height is not retained in the entry.
- sequence_number is zero. Numbers are assigned during output-location processing.
- The block updater processes non-coinbase transactions before coinbase so fee inscriptions can be assigned. Internal processing order is not automatically a protocol claim-priority rule.

P-S04 corroborates matching reference behavior at its independent pinned revision. Matching an implementation is not proof of normative intent. I-01 must use the protocol report and fixed independently sourced vectors to elect the exact interpretation for single/split competing claims, nonzero/sibling vout, multiple inputs, existing offset-zero inscriptions and fee settlement. P-03 then qualifies the chosen profile and carries origin/completion coordinates into P-02.

Decision rule: an unresolved case that can change validity, first claim or ownership blocks publication of the affected profile. Preserve the uncertainty; do not sort by a convenient database number or assume Bitcoin witness behavior. Complete parser changes require targeted regression of the shared authority's existing consumers.

## P-F05: Current APIs do not expose a recoverable rollback generation

Status FAIL for the new feed contract. Existing rollback code improvement is preserved.

Reproduce by inspecting /status, IndexCapabilities and Reorg::handle_reorg. /status exposes unrecoverably_reorged, but the feed needs to detect recoverable rollback too. This fresh revision reads Statistic::Reorgs before restoring a redb savepoint and increments it afterwards in the restore transaction. That existing counter already provides the durable basis for reorgEpoch. /api/v1/capabilities does not expose it, a database generation ID, profile or feed coverage.

Remedy: reuse the counter, add persistent databaseId and expose both in the atomic read. Restore feed tables and watermark together. Rebuilds/external restores rotate generation, and all old cursor/body references reject with 409. Test a fork that catches up to the same height, a rollback during paging, a process restart and restoration of an older database. Ordinary count/hash equality is insufficient.

## P-F06: Existing databases cannot prove historic output/script/value state from current rows

Status FAIL for the required historical feed. Affects P-C04/P-C02 and replay prerequisites.

Reproduce from the schema and update path: spent current rows are removed, reveal txids are retained for inscription reassembly, and transaction retention is optional. Those tables do not retain every historical inscription movement's previous script/value or original reveal block coordinates. The current output APIs do not fill this gap.

Remedy: P-02 captures durable history forward from an explicit coverage boundary, then performs a bounded, resumable history projection replay using the self-hosted archival node and the exact qualified parser. Replay overlapping live coverage must verify identical block/event digests and current ownership before joining the coverage range. A missing archival transaction, anchor or required input value leaves coverage unavailable. Do not deploy a second authoritative Doginals service.

## Research and execution limits

The topology documentation in this baseline describes Doginals on indexers-2 at loopback 8390 and a full index builder elsewhere at 8391. Those are historical repository statements; only the integration owner's fresh runtime inventory may establish deployed binary, flags, tip, lag and endpoint readiness.

No secrets, raw AGENTS.md, credential-bearing launcher snapshots, wallets or index databases belong in evidence. External source snapshots and code-only reproduction artifacts are safe to bundle after scanning. No parser fixes, feed implementation, migration, reindex, restart or release were performed here.
