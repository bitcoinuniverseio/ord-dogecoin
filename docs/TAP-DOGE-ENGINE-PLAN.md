# TAP-DOGE engine: port plan

Status: started 2026-10-17. Branch `feat/tap-doge-engine-20261017`.

## Why

bitcoinuniverse.io reads TAP-DOGE from Trac's P2P reader channel
(`42b96d26...781e`). Only two peers serve that channel, and neither has the
blocks for the global mint, transfer and send lists, filled trades or holder
lists. Reads of those keys time out, so Activity, holder lists and the local
normalized index (stuck at 2026-09-29) cannot complete. A restored 84 GB
reader store did not help: after the fork truncation the newest entries still
have to come from the same peers.

No open-source TAP writer exists for Dogecoin. Trac publishes the Bitcoin
engine as `ord-tap` (CC0, pinned here at `b8f6ea3`, the revision Universe's
Bitcoin `universe-ordtap-writer` runs). This branch ports that engine into
ord-dogecoin so Universe indexes TAP-DOGE from its own Dogecoin node.

## Shape

- Engine: copy `ord-tap/src/index/updater/inscription_updater/tap/**` into
  `src/index/updater/tap/`, move the `impl InscriptionUpdater` blocks onto a
  `TapEngine` owned by the updater, and replace bitcoin 0.32 APIs with the
  0.29 / rust-dogecoin equivalents.
- Hooks: `Origin::New` and `Origin::Old` in `update_inscription_location`,
  the lost-to-fees path, the per-block finalize before the journal write, and
  `hdr/<h>` on the header-only fast path (DMT reads past headers).
- Storage: a `TAP_KV` redb table plus `Statistic::IndexTap`. Reorgs roll back
  through the existing savepoints.
- Rules: a per-chain feature table. Dogecoin values:

| Rule | Bitcoin | Dogecoin |
| --- | --- | --- |
| Dogemap (bitmap) start | 779,832 | 4,771,259 (spec) |
| TAP start | 801,993 | calibrate (first deploy) |
| Full ticker lengths | 861,576 | 5,487,639 (spec) |
| DMT start | 817,705 | calibrate (first dmt-deploy) |
| DMT precise parsing | 885,588 | 5,497,100 (spec) |
| Privilege-auth | 841,682 | from TAP start (calibrate) |
| Value stringify, auth item length, testnet fix | various | calibrate, default off |
| Jubilee / cursed | 824,544 | not applicable |
| NAT rewards, staking, miner shields, trade emergency | various | off |

- Addresses: Base58Check only, mainnet versions 30 and 22.
- Output: a subset of ord-tap's `/r/tap/*` reader routes and the
  `tap-writer-ordtap-v1` export, with the Trac reader key schema byte for
  byte, so index-doge-tap reads it like index-tap reads the Bitcoin writer.

## Replay

TAP state needs every operation since TAP launched, and inscription numbers
need every inscription since the first one. Replay from 4,609,723 in a lean
`--index-tap-only` mode: no sat index, no address tables, store reveal
transactions only for `text/plain*` and `application/json*`. Expected size
150 to 250 GB. Benchmark 50k blocks around 5.0M before choosing SSD or HDD.

## Verification

Diff against the Trac channel at equal heights for the keys its peers still
serve: deployments (`dli/<n>`, `d/<tick>`), `dc/<tick>`, per-ticker mint and
sent lists, holder counts. Bisect the first divergent block to calibrate each
unknown height and record it with its evidence. Dogemap is checked against
Universe's `dogemap-indexer`. Port the ord-tap unit tests in regtest mode and
add Dogecoin boundary tests.

## Phases

| Phase | Work | Effort |
| --- | --- | --- |
| P0 | Dependencies: renamed secp256k1 0.29, sha2, unicode-segmentation, vendored regex crates | 2-3 days |
| P1 | Engine port onto `TapEngine`, bitcoin API rewrite, chain feature table | 2-3 weeks |
| P2 | Hooks, tables, options | 1 week |
| P3 | Dogecoin rules | 3-5 days |
| P4 | Lean mode and benchmark | 1 week |
| P5 | Reader routes, export, index-doge-tap source | 1-2 weeks |
| P6 | Tests, diff harness, full replay, calibration | 2-4 weeks, mostly replay |

## Risks

1. Trac's Dogecoin writer rules are not written down; the channel diff is the
   only oracle and it cannot see dogemap.
2. Doge-only quirks stay invisible until a diff shows them.
3. Inscription numbering depends on the pinned parser profile
   `doginals-trac-1.0.2-compat-v1`.
4. Replay time on HDD is unproven.
5. `auth.rs` (AMM, locks, obligations) must be gated off by feature, not only
   by height.
