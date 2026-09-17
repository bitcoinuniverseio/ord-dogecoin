HTTP API
========

`ord server` serves three overlapping surfaces from one port:

| Surface | Prefix | Stability |
| --- | --- | --- |
| Machine contract | `/api/v1/...` | Designed for downstream services. Bounded, self-describing, exact string quantities, fails closed. |
| Legacy JSON | `/drc20/...`, `/dunes/...`, `/inscriptions/...`, `/utxos/...`, `/address/...` | Inherited from `wonky-ord`. Kept for compatibility. Shapes vary between routes. |
| HTML explorer | everything else | Human browsing, plus `/content` and `/preview` for inscription bytes. |

The machine-readable contract is [`openapi.yaml`](https://github.com/bitcoinuniverseio/ord-dogecoin/blob/develop/openapi.yaml)
at the repository root, declared in `docs.manifest.json`. Load it into any
OpenAPI 3.0 tool.

There is **no authentication and no rate limiting**. See
[Security](security.md).

The v1 machine contract
-----------------------

These endpoints exist because the legacy routes cannot safely be built on. They
follow four rules:

1. **Every quantity is a string.** DRC-20 and Dunes supplies are `u128`. A JSON
   number cannot carry one without rounding. `max_atomic`, `supply_atomic`,
   `amount_atomic` and every sibling are decimal strings of exact digits.
2. **Every page is bounded and ordered.** `limit` has a hard maximum,
   `cursor` is a decimal offset over a deterministic order, and `total_count`
   reports the full size before the bound. A page boundary can neither repeat
   nor skip an item at a given indexed height.
3. **Every response names its checkpoint.** `chain`, `block_count` and
   `block_hash` are on every payload, so a consumer always knows which indexed
   state it read.
4. **Missing capability is an error, not an empty list.** An index built
   without `--index-drc20` returns HTTP 400 with an actionable message rather
   than `200 []`, which downstream is indistinguishable from a chain that
   genuinely has no tokens.

### `GET /api/v1/capabilities`

Reports what this database can answer. No parameters.

```json
{
  "chain": "dogecoin",
  "block_count": 5764321,
  "block_hash": "...",
  "drc20": true,
  "dunes": true,
  "sats": false,
  "transactions": true,
  "network": "mainnet",
  "drc20Decisions": true,
  "drc20DecisionsFromHeight": 5700000
}
```

The four booleans are the flags stored **in the database file at creation
time**, not the flags on the command line. `network` is the configured
chain (`mainnet`, `testnet`, `regtest` or `signet`), so a consumer can
verify which network it is reading instead of inferring one from a port.
`drc20Decisions` says whether per-operation DRC-20 verdicts are retained
(true whenever `drc20` is), and `drc20DecisionsFromHeight` is the first
height they are retained from, or `null` until the first block has been
recorded (the operations route below explains coverage). This is the endpoint to call before
trusting any other one, and the endpoint to poll for liveness: `block_count` is
the number of indexed blocks, so the indexed tip height is `block_count - 1`.

It also confirms you are talking to a Dogecoin index and not another chain's
`ord` instance on a neighbouring port.

### `GET /api/v1/inscriptions`

| Parameter | Type | Default | Bound |
| --- | --- | --- | --- |
| `cursor` | integer | latest | the value returned as `next_cursor` on the previous page |
| `limit` | integer | 250 | 1 to 1000 |

Walks inscriptions newest first. Each item carries the inscription id and
number, genesis height, timestamp, resolved content type and length,
`subsidy_sats` for its genesis height, and its current location (txid, vout,
offset, value, script and address) or `null` when it sits on lost value.

The payload also carries `subsidy_schedule_hash`: the SHA-256 of the raw
`subsidies.json` bytes at `SUBSIDIES_PATH`, read at request time. Two indexes
that report different hashes were built from different subsidy schedules and
will disagree about ordinal numbers. Compare it before merging data from two
instances. If `SUBSIDIES_PATH` is unset the endpoint returns 400.

When an inscription delegates its content, `content_type` and `content_length`
describe the **delegate's** bytes, which is what `/content` will actually serve.

### `GET /api/v1/inscriptions/{inscription_id}`

One inscription in the field layout upstream `ord` answers for
`GET /inscription/{id}` under `Accept: application/json`, so a consumer written
against upstream reads this fork unchanged. The same document is what
`GET /inscription/{inscription_id}` and `GET /shibescription/{inscription_id}`
answer when the request carries `Accept: application/json`; without that header
those two routes keep serving HTML for browsers.

```json
{
  "chain": "dogecoin",
  "network": "mainnet",
  "id": "<txid>i0",
  "number": 12345,
  "address": "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq",
  "content_type": "text/plain;charset=utf-8",
  "content_length": 42,
  "height": 4600000,
  "fee": 2500000000,
  "value": 100000,
  "sat": null,
  "satpoint": "<txid>:0:0",
  "output": "<txid>:0",
  "genesis_transaction": "<txid>",
  "timestamp": 1700000000,
  "charms": [],
  "parents": [],
  "child_count": 0,
  "rune": null,
  "metaprotocol": null,
  "previous": "<txid>i0",
  "next": null
}
```

- `height`, `fee` and `timestamp` describe the genesis block and transaction;
  `value`, `satpoint`, `output` and `address` describe the current location.
  `address` is `null` when the current output is not an address, `sat` is
  `null` without `--index-sats`, and `previous` and `next` are the ids of the
  inscriptions numbered one lower and one higher, or `null`.
- `charms`, `parents`, `child_count`, `rune` and `metaprotocol` exist for
  layout compatibility with upstream and are always empty, `0` or `null`: this
  fork does not index them.
- This route is the one exception to rule 1. Its integers are exact JSON
  numbers because upstream answers them that way and its consumers parse them
  from the JSON source text. `serde_json` prints a `u64` without rounding.
- An unknown or malformed id is `404 {"error":"inscription not found"}`.

### `GET /api/v1/outputs/{outpoint}`

One transaction output in the field layout upstream `ord` answers for
`GET /output/{outpoint}` under `Accept: application/json`. The same document is
what `GET /output/{outpoint}` answers when the request carries
`Accept: application/json`; without that header the route keeps serving HTML.

```json
{
  "chain": "dogecoin",
  "network": "mainnet",
  "outpoint": "<txid>:0",
  "address": "D6VhYBz1fKqA4A3nQrVZqfDkFvX2F4j3Zq",
  "indexed": true,
  "inscriptions": ["<txid>i0"],
  "runes": { "UNIVERSE•DUNE": { "amount": 1000, "divisibility": 0, "symbol": null } },
  "sat_ranges": null,
  "script_pubkey": "OP_DUP OP_HASH160 <hash> OP_EQUALVERIFY OP_CHECKSIG",
  "spent": false,
  "transaction": "<txid>",
  "value": 100000
}
```

- `inscriptions` are the ids currently on the output; a spent output has none.
  `runes` carries the Dunes balance under the upstream key. `sat_ranges` is
  `null` without `--index-sats`. `spent` comes from the unspent-output table
  of the index, not from the node.
- Like the inscription detail, this route answers exact JSON numbers because
  upstream does and its consumers parse them from the JSON source text.
- An unknown transaction, a vout beyond the transaction, or a malformed
  outpoint is `404 {"error":"output not found"}`.

### `GET /api/v1/drc20/tokens`

| Parameter | Type | Default | Bound |
| --- | --- | --- | --- |
| `cursor` | decimal string | `0` | offset over ticker order |
| `limit` | integer | 250 | 1 to 1000 |

The DRC-20 deployment catalog: ticker, deploy inscription id and number,
decimals, `max_atomic`, `limit_atomic`, `minted_atomic`, `remaining_atomic`,
`holder_count`, deployment height and timestamp, deployer, latest mint number,
and whether minting is complete.

This is deliberately **not** the transferable inventory. A valid deployment
with no outstanding transferable still appears here, so a downstream token
index built on this endpoint cannot silently drop real tokens.

Every payload carries `drc20_index_enabled`, so an empty catalog is never
ambiguous.

### `GET /api/v1/drc20/tokens/{tick}`

One deployment, same item contract as the catalog.

### `GET /api/v1/drc20/tokens/{tick}/holders`

| Parameter | Type | Default | Bound |
| --- | --- | --- | --- |
| `cursor` | decimal string | `0` | offset over holder order |
| `limit` | integer | 250 | 1 to 1000 |

Holder balances for one ticker in atomic units, split into `overall_atomic`,
`transferable_atomic` and `available_atomic`.

### `GET /api/v1/drc20/operations/{inscriptionId}`

Was this exact DRC-20 operation accepted or rejected by the ledger, and why.

```json
{
  "inscriptionId": "<txid>i0",
  "txid": "<txid>",
  "index": 0,
  "operation": "mint",
  "tick": "abcd",
  "amount": "10",
  "verdict": "accepted",
  "reason": null,
  "ruleset": "drc20-v1",
  "checkpoint": { "height": 5700123, "blockHash": "..." },
  "reorgEpoch": 0,
  "coverage": { "decisionsFromHeight": 5700000, "indexedHeight": 5700400 }
}
```

The verdict is written in the **same write transaction** as the ledger
change it explains, by the same code path that applied or refused the
operation, so balances and verdicts can never disagree. A reorg that
restores a savepoint rolls both back together; re-indexing re-derives both.

| `verdict` | Meaning |
| --- | --- |
| `accepted` | The ledger applied this operation. `amount` is the ledger-effective atomic amount for mint (after the supply cut-off), inscribe-transfer and transfer; `null` for deploy. |
| `rejected` | The ledger refused it. `reason` is the protocol error, for example `amount exceed limit: 11`, `tick: zzzz not found`, `insufficient balance: 10 100`, `tick: abcd has been existed`. Nothing changed. |
| `not-evaluated` | No verdict is retained. `reason` is `drc20-index-disabled` (database created without `--index-drc20`), `outside-decision-coverage` (the block predates `decisionsFromHeight`) or `not-a-drc20-operation` (the block was evaluated and the inscription carries no DRC-20 operation). `operation`, `tick` and `amount` are `null`; `checkpoint` is the inscription's block. |

`not-evaluated` is neither acceptance nor rejection. A consumer must not read
the ledger backwards to invent a verdict for it, and must not treat an
inscription's `metaprotocol` marker as acceptance.

`404` means the inscription is not indexed at all.

An inscription carries at most two operations: its own deploy, mint or
inscribe-transfer (whose `txid` is the inscription's txid, which is what this
route answers) and, for an inscribe-transfer, its first transfer in a later
transaction. The transfer's verdict is listed by the collection route.

`reorgEpoch` counts the savepoint rollbacks the index had performed when the
verdict was recorded. A verdict re-derived after a rollback carries a higher
value than the one it replaced; a verdict below the fork point that survived
inside the restored savepoint keeps its value, because it was not
re-evaluated.

### `GET /api/v1/drc20/operations?txid={txid}`

Every retained verdict for the operations one transaction carried, as
`{ "txid", "decisions": [...], "coverage": {...} }`. A transaction can
inscribe one operation and spend several inscribe-transfer inscriptions at
once; each is a separate record. An empty `decisions` list is not a
rejection: `coverage` says whether the index could have retained one.
Returns `400` without a valid `txid`, or on a database created without
`--index-drc20`.

#### Coverage without a rebuild

An existing database gains the decision table on its first indexed block
after the upgrade, and records that height as `decisionsFromHeight`. Nothing
before it is backfilled, because the historical ledger state needed to
re-decide those operations no longer exists; those operations are reported as
`outside-decision-coverage` rather than inferred from today's balances.

### `GET /api/v1/drc20/transferables`

Every outstanding DRC-20 transferable inscription with its ticker, atomic
amount, owning address and location. Answers "what can be spent right now".
Unlike the catalog it is not paginated.

### `GET /api/v1/dunes/tokens` and `GET /api/v1/dunes/tokens/{dune}`

The dune catalog and single-dune lookup. Same cursor and limit semantics,
ordered by etching. Items carry the spaced name, the `block:index` identifier,
number, symbol (null when none was etched), `divisibility`, etching txid,
`supply_atomic`, `premine_atomic`, `mints_atomic`, `burned_atomic`, etched
height and timestamp, and `mintable` (whether the terms allow a mint in the
next block).

`divisibility` is the only rule by which an amount here may be scaled for
display.

`{dune}` accepts either the spaced name or the `block:index` identifier.

### `GET /api/v1/funding/{address}`

| Parameter | Type | Default | Bound |
| --- | --- | --- | --- |
| `limit` | integer | 20 | 1 to 50 |

Confirmed **cardinal** UTXOs for one address: outputs carrying inscriptions or
Dunes are excluded. Each item carries the exact atomic value, script,
confirmation count, and the **raw previous transaction**, so a caller can
verify the prevout independently rather than trusting this index.

The address must be in its exact encoding; a re-encoded or differently cased
form is rejected with `funding address must use its canonical encoding`.

`total_count` is the complete cardinal UTXO count before the response bound and
`truncated` says whether more exist. `inventory_complete` describes index
completeness and **must not** be read as "this is the whole set".

Two warnings:

- This endpoint requires `--index-transactions`.
- The index has no mempool. An output spent by an unconfirmed transaction still
  appears here. Callers must apply their own reservations before treating an
  output as spendable. See [Reorgs and mempool](reorgs.md#mempool-there-isnt-one).

### Errors

| Status | When |
| --- | --- |
| 400 | Bad limit, bad cursor, non-exact address encoding, or a missing index capability. |
| 404 | Unknown ticker, dune, inscription, or an index with no chain tip yet. |

The capability messages name the flag, say it is fixed at database creation,
and say a rebuild is required, because an operator reading the message cannot
fix it by restarting with the flag added:

```
this index was created without --index-drc20 and cannot serve DRC-20 state;
the flag is fixed at database creation and requires a rebuild
```

Legacy JSON routes
------------------

Inherited from `wonky-ord` and kept working. Useful, but their shapes are not
governed by the four rules above: quantities may be JSON numbers, and pages are
route-specific.

| Route | Notes |
| --- | --- |
| `GET /block-count` | Plain-text count of indexed blocks. |
| `GET /status` | Plain text. **Always HTTP 200.** Body is `OK`, or `unrecoverable reorg detected, please rebuild the database.` |
| `GET /tx/{txid}` | Transaction, `?json=true` for JSON. |
| `GET /output/{outpoint}`, `GET /outputs/{list}` | Output detail. |
| `GET /address/{address}` | Outputs held by an address. |
| `GET /utxos/balance/{address}[/{page}]` | `?limit=`, `?show_all=`, `?show_unsafe=`, `?value_filter=`. |
| `GET /inscriptions/balance/{address}[/{page}]` | Inscriptions held by an address. |
| `GET /inscriptions/validate?inscription_ids=&addresses=` | Bulk ownership check. |
| `GET /shibescriptions_on_outputs`, `GET /shibescriptions_by_outputs` | Inscriptions on a list of outputs. |
| `GET /drc20/tick`, `GET /drc20/ticks`, `GET /drc20/tick/{tick}` | Token info; `?show_holder=true` on the last. |
| `GET /drc20/tick/holder/{tick}` | Holders for a ticker. |
| `GET /drc20/balance/{address}[/{page}]` | `?tick=`, `?show_utxos=`, `?value_filter=`. |
| `GET /drc20/validate` | DRC-20 validity check. |
| `GET /dunes`, `GET /dune/{dune}`, `GET /dunes/balances` | Dune catalog and balances. |
| `GET /dunes/balance/{address}[/{page}]` | `?show_all=`, `?list_dunes=`, `?filter=`. |
| `GET /dunes_on_outputs` | Dune balances for a list of outputs. |
| `GET /blocks/{start}/{end}` | Block range; `?no_inscriptions=`, `?no_input_data=`. |
| `GET /sat/{sat}`, `GET /range/{start}/{end}`, `GET /rare.txt` | Require `--index-sats`. |

`/shibescription`, `/shibescriptions` and `/shibescriptions/{from}` are
Dogecoin-flavoured aliases of the `/inscription` and `/inscriptions` routes.

Explorer and content
--------------------

| Route | Purpose |
| --- | --- |
| `GET /` | Explorer home. |
| `GET /inscription/{id}`, `/inscriptions`, `/inscriptions/{from}` | Inscription pages. |
| `GET /block/{height or hash}`, `/tx/{txid}`, `/output/{outpoint}`, `/input/{block}/{tx}/{input}` | Chain pages. |
| `GET /content/{id}` | The inscription's raw bytes, with its own content type. |
| `GET /preview/{id}` | A sandboxed preview page for the content type. |
| `GET /search`, `/search/{query}` | Redirects to whatever the query identifies. |
| `GET /feed.xml` | RSS feed of recent inscriptions. |
| `GET /static/{path}`, `/favicon.ico` | Embedded assets. |
| `GET /faq`, `/bounties` | Redirects into this handbook. |

`/content` and `/preview` serve attacker-supplied bytes. Read
[Security](security.md#serving-inscription-content) before exposing them.

Response headers
----------------

Applied to every response:

| Header | Value |
| --- | --- |
| `Content-Security-Policy` | `default-src 'self'`, unless already set by the handler. `--csp-origin` changes what content routes send. |
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains; preload` |
| `Access-Control-Allow-Origin` | `*`, methods `GET` only |
| `Content-Encoding` | gzip or brotli, by negotiation |

`Access-Control-Allow-Origin: *` is unconditional. Any web page can read this
server from a browser. That is intended for a public explorer and is a problem
for an instance you thought was private.
