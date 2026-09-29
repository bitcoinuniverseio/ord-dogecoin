# P-01 shared Doginals feed contract

Status: PROPOSED implementation contract agreed with dogemap-indexer I-02 on 2026-09-29. No endpoint below is implemented by this preparation. This is an internal integration design; Dogemap validity, activation and duplicate priority remain governed by I-01's pinned protocol profile.

## Routes and identity

| Method and route | Purpose |
| --- | --- |
| GET /api/v1/dogemap-feed/capabilities | Current feed identity, coverage, limits and readiness |
| GET /api/v1/dogemap-feed/blocks/:height | Paged immutable events for one explicit block |
| GET /api/v1/dogemap-feed/inscriptions/:inscription_id/body | Bounded intrinsic body chunks tied to creation block identity |

The first block page requires blockHash, databaseId and reorgEpoch copied from a verified request context; subsequent pages additionally carry cursor and optional limit. A height/hash is checked against the Universe Dogecoin node by I-03 and against the provider's retained block manifest here. Node confirmation alone does not prove feed coverage.

Identity tuple: chain, network, genesisHash, providerCommit, parserProfile, orderProfile, databaseSchema, databaseId, feedVersion. A checkpoint is height, blockHash and reorgEpoch. The first three identify an explicit chain; chain is always dogecoin. Supported production/test identities are mainnet/testnet/regtest with the exact genesis hash from the pinned Dogecoin Core source. Do not accept a Bitcoin network or infer a network from a port.

All chain heights, inscription numbers, event/order positions, amounts, offsets, byte lengths, event cardinalities and reorg counters crossing this API are exact decimal JSON strings. Bounded page limits and capability maxPageLimit/maxResponseBytes/maxInlineBodyBytes/maxBodyChunkBytes are ordinary safe JSON integers; validate their small explicit ranges. Nonnegative values use 0 or a nonzero leading decimal digit followed by digits, with no whitespace, exponent, sign or leading zero. Inscription number signedness follows parserProfile and is checked independently. All hash hex is lowercase with fixed length. Empty, missing and zero are distinct.

## Capabilities fields

| Field | Type / invariant |
| --- | --- |
| chain / network / genesisHash | Explicit expected chain identity |
| providerCommit | Full build commit recorded by build provenance, not mutable branch name |
| parserProfile / orderProfile | Named immutable versions qualified in P-03 and elected by I-01 |
| databaseSchema / databaseId / feedVersion | Versioned database format; persistent generation ID; wire contract version |
| indexedCheckpoint | null before any fully retained feed block; otherwise height/blockHash/reorgEpoch read atomically |
| coverageFromHeight | null before contiguous coverage exists; actual retained lower bound, never guessed activation |
| coverageRanges | Verified inclusive contiguous segments while replay is incomplete; not a substitute for global readiness |
| indexTip | Ordinary authority's tip checkpoint, separately named; not the feed watermark |
| ready / unavailableReason | Readiness and typed reason; ready requires qualified profile, integrity and required coverage |
| unrecoverablyReorged / recovering | Explicit unavailable/recovery states; never hidden by a successful HTTP status |
| maxPageLimit / maxResponseBytes | Bounded event count and response size |
| maxInlineBodyBytes / maxBodyChunkBytes | Deterministic inline threshold and bounded chunk length |
| bodyRangePolicy | base64-chunks-v1 for the body route defined below |
| eventsHashAlgorithm | sha256-rfc8785-dogemap-feed-v1, tied to the exact descriptor projection below |

Read identity, watermark, corresponding hash, coverage and Statistic::Reorgs from one redb read transaction. Existing block_count is height+1; never copy it into indexedCheckpoint.height. P-02's existing reorg counter supplies reorgEpoch, combined with databaseId to detect rebuild/restore generation reuse. This is the provider's source epoch. dogemap-indexer independently maintains its MySQL projection checkpoint.reorgEpoch and exposes this source counter under provider.reorgEpoch; never equate the two counters.

## Block page fields

Every success page includes the identity tuple, height, blockHash, parentHash, reorgEpoch, indexedCheckpoint, coverageFromHeight, events, totalEvents, eventsHash, nextCursor and complete. The block header identity, semantic events, total and digest are immutable within a qualified profile and block hash.

Events occupy dense eventOrdinal values 0 through totalEvents minus one. A cursor is opaque to consumers and binds the identity tuple, requested height/hash/epoch, eventOrdinal, manifest digest and cursor format. Enforce bounded size, strict decoding, integer range and full tuple equality. The cursor is a read position, never authorization; existing access controls still apply. Return typed 409 when a previously valid cursor refers to a replaced generation/fork/profile/manifest.

Only a final page has complete=true and nextCursor=null. Earlier pages have complete=false and a strictly advancing cursor. A one-page zero-event response is truthful only when a stored complete zero-event manifest covers that exact block, the feed watermark is at least its height, and the request identity is current. Missing data, lag, profile uncertainty, storage errors and an incomplete historical replay are never empty success.

The consumer must accumulate and validate the entire block before committing any Dogemap decision or checkpoint. It checks tuple equality on every page, dense ordinal sequence, exact totalEvents, complete termination and eventsHash. It then rechecks its node/provider fork anchor before advancing. Retry duplicates are idempotent. Same-height hash changes force I-05 rewind and replay.

## Event fields

Common fields: type (creation or transfer), eventOrdinal, id, inscriptionNumber, txid, txIndex, inscriptionIndex and orderKey. id is the profile's exact inscription ID; it is never reconstructed as completionTxid+i0 unless the qualified profile specifies that identity. orderKey is the explicit tuple determined by orderProfile, including applicable completion/reveal and fee-finalization semantics. It does not inherit HashMap iteration or the current entry.sequence_number field, which is zero in this baseline.

| Creation field | Meaning |
| --- | --- |
| originTxid / originHeight / originBlockHash / originTxIndex | The first reveal coordinates |
| completionTxid / completionHeight / completionBlockHash | The transaction/block where intrinsic assembly became complete |
| txIndex / inscriptionIndex | Completion transaction coordinate and profile inscription position |
| rawBody | null for absent body; otherwise intrinsic bytes descriptor below |
| contentTypeBase64 | Original content-type bytes, nullable; no string normalization or delegated replacement |
| delegate | null or rawBase64 plus decoded id when valid; provenance only, no resolution for ingestion |
| location | Assigned/lost location at that event, after required fee settlement |

Transfer additionally includes fromLocation and toLocation. Historical output scripts and values must be captured at ingestion; fetching the inscription's current location later cannot reconstruct the previous event. The transaction which spends/reveals an inscription may differ from the coinbase transaction ultimately receiving its fees. Preserve both event transaction coordinates and the actual location output. Do not sort a fee settlement ahead of its originating event because the coinbase happens to be transaction zero.

A Location contains status (assigned, lost or unresolved), outpoint, offset, valueKoinu, scriptPubKeyHex and address. Assigned outpoint is exact txid:vout, offset is within output value, and scriptPubKeyHex is raw script bytes. Address is nullable display metadata derived for the explicit network. Lost means the provider's qualified lost-sat state with no spendable output; retain its offset/provenance. Unresolved means required data is unavailable and prevents complete feed output. A null address does not mean lost or unowned. A non-address or unspendable script remains distinguishable by its raw script.

## Intrinsic content and bounds

rawBody is encoding=base64, byteLength, sha256, bytes and bodyRef. byteLength and digest cover the exact intrinsic body. bytes is base64 when inlined; otherwise null. bodyRef is a same-origin relative route for the exact inscription and its creation block hash, databaseId and reorgEpoch. No arbitrary host, redirect to an unapproved origin or delegated /content fallback is allowed.

Use the capability's fixed maxInlineBodyBytes threshold independently of requested page length. Larger content stays out of the event response, with a durable referenced object. All body objects required to describe a complete manifest must be retained and hash-verifiable. An individual response remains bounded. Consumer rejection by body length is permitted only if I-01 explicitly pins that validation rule; inability to retrieve required content is BLOCKED, not an invalid claim.

The body route requires blockHash, databaseId, reorgEpoch, offset (default 0) and length within maxBodyChunkBytes. It returns exact identity, id, encoding=base64, offset, length, byteLength, bytes, sha256, nextOffset and complete. Validate every chunk's range and identity and the reassembled body's length/digest before parsing. A stale generation returns 409. Required missing bytes return unavailable status, not hidden-content HTML, zero-length substitution or a delegate's bytes. Do not hold redb transactions across external network calls.

## Digest projection

Use the RFC 8785 serialization rules only for deterministic internal API hashing (P-S05). All integers are strings, so integer precision does not depend on a JSON number implementation. Reject duplicate object keys, malformed Unicode and malformed encoded byte strings at the boundary.

eventsHash is lower-case SHA-256 of RFC 8785 UTF-8 bytes for this exact object: feedVersion, parserProfile, orderProfile, network, genesisHash, height, blockHash, parentHash, events. events is the complete block array in eventOrdinal order. Each descriptor includes all event fields defined above, except rawBody.bytes and rawBody.bodyRef. The descriptor retains body byteLength/sha256/encoding, so inline thresholds and transport URLs do not alter semantic digests. Optional semantic fields are present with null, not inconsistently omitted. A future added semantic field requires a new feed/hash schema version.

Exclude databaseId, reorgEpoch, providerCommit, databaseSchema, page limits/cursors, readiness/observed time and checkpoint watermarks from the hashed object; these bind request validity separately and can change while a historical block's semantic events remain the same. Do not hash a page as though it were a complete block. Pin a cross-language golden vector in provider P-01 and indexer I-02.

## Errors and consumer effects

| Status / code | Required meaning and consumer action |
| --- | --- |
| 400 invalid_request / invalid_cursor | Malformed or out-of-range parameters; do not advance |
| 409 snapshot_replaced | Wrong/stale database, epoch, block hash, profile or manifest; recheck anchors and rewind as needed |
| 404 unknown_inscription | Requested ID absent at a proved covered snapshot; not evidence that an uncovered block contains no candidates |
| 503 coverage_unavailable | Requested history incomplete or watermark below required height; bounded retry, no decision |
| 503 provider_recovering | Recoverable reorg/reconciliation active; retain last accepted consumer checkpoint |
| 503 provider_unrecoverable | Authority cannot safely serve; operator recovery required |
| 503 content_unavailable / integrity_failure | Required body/event data missing or corrupt; quarantine and no claim progress |

Availability failures are observable with correlation ID, requested block identity and reason, without secrets or body contents in logs. An HTTP 200 by itself is not a completeness proof. Keep the old live inventory, output and content contracts intact for existing users; Dogemap ingestion uses this qualified feed.
