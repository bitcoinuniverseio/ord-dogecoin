# Dogecoin implementation handoff - preparation only

Read `D:\universe\AGENTS.md` and applicable repository instructions.

This file and the IMPLEMENTATION-HANDOFF source comments are non-executable preparation, not a claim of implemented functionality, passing end-to-end acceptance or public release. Do not publish internal engineering instructions as public product documentation.

## Exact starting state

Repository: ord-dogecoin

Worktree: `D:/universe/ord-dogecoin/.worktrees/doge-prep-20260919-0438`

Branch: `prep/doge-20260919-0438`

Source baseline: `f7d1e17ded3ae136cfd17bf53e62d298035bcc90`

The local preparation commit and patch are recorded in `prepared-baselines.json` in the verified handoff archive at `D:/universe/core/audits/implementation-prep-20260919-0438-doge`. Open its `IMPLEMENTATION_PROMPT.md`, `README.md`, `annotation-index.json`, `work-packages.json` and `coverage-matrix.json`. Do not reconstruct the scope from the older 0220 audit.

## Local annotation index

- DG-A-025 / DG-WP-07 / `src/subcommand/server.rs` / `async fn drc20_token_inventory(`
- DG-A-026 / DG-WP-11 / `src/drc20/tick.rs` / `pub const TICK_BYTE_COUNT: usize = 4;`

## Execution order and safeguards

Use the dependency graph in the current work packages. Preserve current executable behavior and concurrent work until implementation begins. Check targeted baseline drift, resolve owned-authority and protocol-semantic prerequisites, implement the complete ledger/network/event contracts, wire native signing and public flows, then verify actual Dogecoin TESTNET outcomes and dependent regressions. Do not use Mainnet test transactions. Enable and release publicly on Mainnet only after functional acceptance and production configuration checks.

No comment or local test replaces transaction/indexer/persistence/UI evidence. Existing CI/type/lint failures in the handoff must be repaired without suppressing tests or weakening contracts. Use only the existing authorized deployment process, preserve legitimate authentication and ownership gates, and retain rollback and uncertain-transaction recovery.

Temporary engineering remarks may be removed only when their mapped acceptance evidence is complete; preserve their history and the verified handoff. Do not let a repository cleanup, worktree prune or unrelated merge remove this active prepared baseline.
