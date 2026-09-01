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
  "transactions": true
}
```

The four booleans are the flags stored **in the database file at creation
time**, not the flags on the command line. This is the endpoint to call before
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
