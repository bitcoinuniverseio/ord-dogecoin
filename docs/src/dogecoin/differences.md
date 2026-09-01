Differences from Bitcoin ord
============================

Dogecoin is not Bitcoin. Almost every operational instinct carried over from
running `ord` on Bitcoin gives the wrong answer here. This page lists the
differences that change behaviour, and points at the code that implements each
one, so you can check any of them yourself.

Summary
-------

| | Bitcoin `ord` | This fork |
| --- | --- | --- |
| Block target | 10 minutes | 1 minute |
| Inscription location | Taproot witness (`tapscript`) | `scriptSig` of input 0 |
| Multi-part inscriptions | One reveal transaction | Chained across consecutive transactions |
| Inscription content limit | Enforced ceiling on mainnet | None on any chain (`Chain::inscription_content_size_limit`) |
| Subsidy schedule | Halving every 210,000 blocks | Per-block table for heights 0 to 144,999, then a fixed schedule |
| Subsidy source | Compiled in | Loaded at runtime from `SUBSIDIES_PATH` and `STARTING_SATS_PATH` |
| Mainnet RPC port | 8332 | 22555 |
| Default first inscription height | 0 | 4,600,000 (mainnet) |
| Address encoding | Base58 `1`/`3`, bech32 `bc1` | Base58 only |
| Extra protocols indexed | Runes | Dunes, DRC-20 |
| Wallet subcommands | Supported | Non-functional, see below |

A confirmation here is worth roughly a tenth of a Bitcoin confirmation
---------------------------------------------------------------------

Dogecoin targets one block per minute; Bitcoin targets one per ten. Depth in
blocks is not depth in work or in elapsed time.

The practical consequences:

- **Six confirmations is about six minutes, not an hour.** Do not reuse a
  Bitcoin confirmation policy verbatim. If your rule was "wait an hour", the
  equivalent depth on Dogecoin is roughly sixty blocks, not six.
- **Heights are large and grow fast.** Dogecoin passes 500,000 additional
  blocks a year. Anything storing a height needs the range, and anything
  logging progress in blocks per second should be read against a chain tip
  that moves ten times as often.
- **The reorg window is short in wall-clock terms.** This indexer can recover
  from a reorg of roughly 40 blocks, which on Dogecoin is roughly 40 minutes of
  chain. See [Reorgs and mempool](reorgs.md).
- **The Universe protocol registry records
  `settlementMinConfirmations: 1`** for both `doginals` and `drc20`, with the
  policy: "Settlement requires the exact Dogecoin transaction confirmation and
  the expected protocol-state transition in a later authoritative checkpoint."
  One confirmation is the floor, and it is paired with a protocol-state check
  in a later checkpoint precisely because one Dogecoin block is thin evidence
  on its own.

Inscriptions live in `scriptSig`, not in a witness
--------------------------------------------------

> The envelope format itself is protocol material, and it is described as
> protocol at
> [TAP on Doge](https://bitcoinuniverseio.github.io/tap-on-doge/). This page
> describes what **this implementation** does with it. Where the two overlap,
> they agree; where you need the rule rather than the code, go there.

Dogecoin has no SegWit and no Taproot, so there is no witness to hide an
inscription envelope in. `src/inscription.rs` parses the envelope out of
`input[0].script_sig` of the transaction, using the same
`OP_FALSE OP_IF "ord" ... OP_ENDIF` shape.

`InscriptionParser::parse` reads, in order: the `ord` protocol tag, a piece
count, and the content type. Each subsequent piece is preceded by the number of
pieces still to come, counting down, and the parser rejects the envelope if
that countdown ever skips.

Two things follow.

**Inscription data is in the transaction body, so it costs body-weight fees.**
There is no witness discount. A Dogecoin inscription pays full rate for every
byte. The offsetting factor is that Dogecoin's fee market is denominated in
DOGE and is normally far cheaper per byte, which is why typical Doginals are
much larger than typical Bitcoin inscriptions.

**Large inscriptions are chained across transactions.** A Bitcoin script push
is capped at 520 bytes, and a `scriptSig` cannot hold an arbitrarily long
script. `Inscription::append_reveal_script_to_builder` chunks the body into
520-byte pushes, and `InscriptionParser::parse` takes a *list* of `scriptSig`s
and walks it: each envelope declares how many pieces remain (`npieces`), and
the parser follows the chain of transactions until the count reaches zero.

While a chain is incomplete the parser returns `ParsedInscription::Partial`.
The indexer then does exactly two things: it moves the partial marker in
`PARTIAL_TXID_TO_INSCRIPTION_TXIDS` forward to the new txid, and it stores the
raw transaction so the content can be reassembled later.

**A partial reveal indexes as nothing.** No inscription entry is written, no
inscription number is assigned, no satpoint is recorded, and no API route will
report it. It becomes an inscription only when the transaction carrying the
final piece confirms. A Bitcoin `ord` index has no equivalent state, because a
Bitcoin reveal is atomic.

There is no content size ceiling
--------------------------------

`Chain::inscription_content_size_limit()` returns `None` for every chain in
`src/chain.rs`. Upstream `ord` enforces a limit on Bitcoin mainnet. This fork
enforces none, on any network.

This is the single most important sizing fact for an operator. A multi-megabyte
Doginal is legal, it will be indexed, its bytes will be stored, and
`/content/:inscription_id` will serve them. Plan disk and bandwidth for content
that is orders of magnitude larger than a Bitcoin inscription corpus of the
same inscription count.

The subsidy schedule is a data file, not a formula
--------------------------------------------------

Bitcoin's subsidy is `50 BTC >> (height / 210_000)`. Dogecoin's first 145,000
blocks paid randomized rewards, and `subsidies.json` in this repository records
a separate value for each of them. Consecutive entries differ by orders of
magnitude, so no formula reproduces them.

`src/epoch.rs` treats **every one of the first 145,000 blocks as its own
epoch**, then adds epochs starting at heights 145,000, 200,000, 300,000,
400,000, 500,000 and 600,000. Epoch 145,005 is the last one and continues
forever, because Dogecoin's subsidy stops halving.

Both tables are loaded lazily, the first time a subsidy or a starting ordinal
is needed, from JSON files named by environment variables:

- `SUBSIDIES_PATH` points at `subsidies.json`, a map of epoch index to subsidy
  in atomic units.
- `STARTING_SATS_PATH` points at `starting_sats.json`, the first ordinal number
  of each epoch.

Both are `expect()`ed. **If either variable is unset or the file is missing,
the process panics the first time it needs a subsidy.** They are not optional.
See [Configuration](configuration.md#required-environment-variables).

The files ship in the repository root. They are consensus-relevant input: two
nodes with different `subsidies.json` will disagree about ordinal numbers.
`/api/v1/inscriptions` returns a `subsidy_schedule_hash` field so a consumer
can detect that disagreement rather than silently inherit it.

Addresses
---------

`src/chain.rs` maps `Chain::Mainnet` to `Network::Bitcoin` of the patched
`rust-dogecoin` crate pinned in `Cargo.toml`. In that crate the mainnet version
bytes are `0x1e` for pay-to-pubkey-hash and `0x16` for pay-to-script-hash,
which produce Dogecoin's `D...` and `A.../9...` Base58 addresses. Testnet uses
`0x71` and `0xc4`.

There is no bech32 address form on Dogecoin and no Taproot output type. Code
that branches on address type from a Bitcoin ord integration will need
adjusting.

One atomic unit is one hundred-millionth of a DOGE. The `COIN_VALUE` constant
is `100_000_000`, the same number as Bitcoin, but a DOGE is not a BTC. The
handbook's "satoshi" is this unit throughout, and the code uses the same word.

The wallet subcommands do not work on Dogecoin
----------------------------------------------

`Options::dogecoin_rpc_client_for_wallet_command` requires the node to return
descriptor wallets and then requires exactly two `tr(` (Taproot) output
descriptors, failing with:

```
wallet "ord" contains unexpected output descriptors, and does not appear to be
an `ord` wallet, create a new wallet with `ord wallet create`
```

Dogecoin Core 1.14 has neither descriptor wallets nor Taproot, so this check
can never pass. It also requires Dogecoin Core `1.14.6.0` or newer before it
gets that far.

**Treat `ord wallet ...` as dead code inherited from upstream.** Build
transactions with a Dogecoin-native wallet. Nothing in the indexing or serving
path depends on these subcommands.

Dunes and DRC-20 have no Bitcoin equivalent here
------------------------------------------------

Upstream `ord` indexes Runes. This fork indexes **Dunes** (`src/dunes/`, the
Dogecoin analogue: etchings, edicts, dunestones, varints, terms) and
**DRC-20** (`src/drc20/`, a JSON inscription token protocol with four-byte
tickers).

Both are opt-in at database creation time and both are separate flags. The
Dunes CLI help still carries the upstream warning that the protocol is
pre-alpha and subject to change; that text is accurate and has not been
softened.

Networks other than mainnet are inherited, not exercised
--------------------------------------------------------

`Chain` still offers `testnet`, `signet` and `regtest`. Dogecoin has a testnet
and a regtest; it has no signet, and the signet entry carries Bitcoin's default
port (38332) and Bitcoin's data directory layout.

`docs.manifest.json` declares mainnet only, because mainnet is the only network
this fork runs and verifies. See [Chains and networks](configuration.md#chains-and-networks).
