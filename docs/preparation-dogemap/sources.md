# Provider source and dependency register

Access date for this workstream: 2026-09-29 UTC. Repository sources are exact pinned code evidence. The internal feed contract is an engineering design; it must not be presented as an external Dogemap protocol specification.

| ID | Pinned source | Verified requirement / implication |
| --- | --- | --- |
| P-S01 | bitcoinuniverseio/ord-dogecoin ab2934f3ae027160274ff199fb7a19fd030041a8 | Actual source baseline: current inventory/read transactions, event/state persistence, parser and reorg counter |
| P-S02 | dogecoin/dogecoin e0a1c157791544e818c901bd9341896965afbf9d | src/script/script.h GetOp2 and script builder use following length bytes LE16/LE32; chain parameters define supported networks |
| P-S03 | cberner/redb v2.6.3 -> f4e3eb69dc2a4b4e01fefff0b9e039c6c7ab98f6 | src/transactions.rs read transaction captures one root; savepoint restoration must include feed state |
| P-S04 | Trac-Systems/ord-dogecoin 1ae4f3fe26a2056ec6f4bd57deefee49552ffb85 | Independent Doginals reference behavior for scriptSig, multipart assembly and fee/location ordering; not automatically normative Dogemap rules |
| P-S05 | RFC 8785, published June 2020 | Deterministic JSON serialization for the new feed hash projection; precision-sensitive integers represented as strings |
| P-S06 | apezord/rust-dogecoin 8cbc14efaf77923f051b017348aeaae8731e9aa1 | Cargo.lock pins forked bitcoin crate 0.29.2; preserve DOGE script/transaction types while testing provider changes |
| P-S07 | apezord/rust-dogecoincore-rpc 03933810dec7844ee580c127fa953cba7d73623f | Cargo.lock pins RPC fork 0.16.0; provider RPC compatibility is dependency, no public external blockchain-data fallback |

## Exact URLs

- P-S01 server: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/subcommand/server.rs
- P-S01 authority DTOs: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/authority_api.rs
- P-S01 index/manifest integration: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/index.rs
- P-S01 updater: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/index/updater.rs
- P-S01 inscription tracker: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/index/updater/inscription_updater.rs
- P-S01 parser: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/inscription.rs
- P-S01 reorg: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/index/reorg.rs
- P-S01 Rtx: https://github.com/bitcoinuniverseio/ord-dogecoin/blob/ab2934f3ae027160274ff199fb7a19fd030041a8/src/index/rtx.rs
- P-S02: https://github.com/dogecoin/dogecoin/blob/e0a1c157791544e818c901bd9341896965afbf9d/src/script/script.h
- P-S02 network parameters: https://github.com/dogecoin/dogecoin/blob/e0a1c157791544e818c901bd9341896965afbf9d/src/chainparams.cpp
- P-S03: https://github.com/cberner/redb/blob/f4e3eb69dc2a4b4e01fefff0b9e039c6c7ab98f6/src/transactions.rs
- P-S04 parser: https://github.com/Trac-Systems/ord-dogecoin/blob/1ae4f3fe26a2056ec6f4bd57deefee49552ffb85/src/inscription.rs
- P-S04 tracker: https://github.com/Trac-Systems/ord-dogecoin/blob/1ae4f3fe26a2056ec6f4bd57deefee49552ffb85/src/index/updater/inscription_updater.rs
- P-S05: https://www.rfc-editor.org/rfc/rfc8785
- P-S06: https://github.com/apezord/rust-dogecoin/tree/8cbc14efaf77923f051b017348aeaae8731e9aa1
- P-S07: https://github.com/apezord/rust-dogecoincore-rpc/tree/03933810dec7844ee580c127fa953cba7d73623f

## Snapshot/retrieval evidence

P-S01 selected source snapshots and unchanged Cargo.toml/Cargo.lock are in the SERVER audit baseline/ directory. Preserve their original bytes and hashes; the working tree contains only added source comments and these handoff documents.

P-S02/P-S04 were downloaded by the protocol research workstream, read directly for this audit, and are included in its research/references/ tree with commits/digests in research/protocol-source-manifest.json. Dogecoin script.h lines around GetOp2 were inspected; the provider's current malformed length expressions were separately verified in the fresh Universe revision.

P-S03 source was retrieved from the versioned upstream raw URL on SERVER into external/redb-2.6.3-transactions.rs. GitHub's tag API resolves v2.6.3 to commit f4e3eb69dc2a4b4e01fefff0b9e039c6c7ab98f6 (external/redb-tag.json). Retrieved file SHA-256: 8051e0cd96bf13b74cb8afb392f10048d66fdfbe0b1cd7c672bb890386336b50. ReadTransaction::new captures the data root and shares a transaction guard for table reads. Use one transaction for a response; this does not make separately opened transactions identical.

P-S05 was retrieved via the official RFC Editor. Its deterministic serialization and recommendation to encode large integers as strings support the proposed internal digest scheme. It does not govern Doginals parsing.

P-S06/P-S07 commits are verified from the actual Cargo.lock; their complete implementation was not audited by this focused workstream. They are exact dependency pins for future targeted compatibility checks, not claims that every behavior is proved.

The public web reader could not open several pinned GitHub/docs.rs files; the workstream used exact source snapshots and a successful direct versioned upstream fetch instead. No inaccessible source was cited as read.

## Traceability

| Requirement | Operation | Source | Implementation | Proposed verification |
| --- | --- | --- | --- | --- |
| Stable complete historical block set | Ingest/retry/no-candidate decision | P-S01/P-S03; internal feed contract | P-02 + P-01 | dogemap-feed-contract + I-02/I-05 |
| Intrinsic content provenance | Parse candidate | P-S01/P-S04 | P-02 + P-01 | Chunk/inlining/delegate/digest vectors |
| Historical script/value/offset | Transfer and current owner | P-S01/P-S04 | P-02 | Same-block, multi-input/output and fee/lost vectors |
| Recoverable fork identity | Pause/rewind/replay | P-S01/P-S03 | P-02 + P-01 | Savepoint/cursor/restart/generation vectors |
| Correct push lengths | Doginals parsing | P-S02; discrepancy P-S01/P-S04 | P-03 | Boundary/malformed raw script and transaction vectors |
| Qualified claim order | Competing Dogemap claims | I-01 protocol report + P-S04 as reference | I-01/P-03/P-02 | Split/single/fee/vout corpus with independent expected results |
| Cross-language digest agreement | Page assembly | P-S05 | P-01/I-02 | Shared exact hash vector at multiple page sizes |
