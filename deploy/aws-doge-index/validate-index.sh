#!/usr/bin/env bash
set -Eeuo pipefail

# Post-migration validation for the Dogecoin Ord index.
# Proves deep history, checkpoint-adjacent state and live tip agreement, rather
# than trusting the reported height alone. Read only: it never writes the index.

API=${API:-http://127.0.0.1:8390}
RPC=${RPC:-http://127.0.0.1:22566/}
COOKIE=${COOKIE:-/mnt/doge-index/secrets/ord-rpc.cookie}
MANIFEST=${MANIFEST:-/mnt/doge-index/control/migration-manifest.json}

fail=0
ok()   { printf 'PASS  %s\n' "$*"; }
bad()  { printf 'FAIL  %s\n' "$*"; fail=1; }

caps=$(curl -fsS --max-time 30 "$API/api/v1/capabilities")
height=$(( $(jq -er '.block_count' <<<"$caps") - 1 ))
auth=$(cat "$COOKIE")
tip=$(curl -fsS --max-time 30 --user "$auth" \
  --data-binary '{"jsonrpc":"1.0","id":"v","method":"getblockcount","params":[]}' \
  -H content-type:text/plain "$RPC" | jq -er '.result')

printf 'indexed_height=%s dogecoin_tip=%s behind=%s\n' "$height" "$tip" "$(( tip - height ))"

# 1. Never below the height the migration carried over.
if [[ -s "$MANIFEST" ]]; then
  floor=$(jq -er '.source.indexedHeight' "$MANIFEST")
  (( height >= floor )) && ok "height $height is at or above the migrated checkpoint $floor" \
                        || bad "height $height REGRESSED below the migrated checkpoint $floor"
else
  bad "migration manifest is missing at $MANIFEST"
fi

# 2. Deep history: inscription 100, minted just after the first inscription height.
old=$(curl -fsS --max-time 30 "$API/api/v1/inscriptions?cursor=100&limit=1")
[[ "$(jq -er '.inscriptions[0].inscription_id' <<<"$old")" \
   == "c3e315dcb12d7f4d6d8423b8ad802ebaf7cb2547dab9d13c860b89efd47804cei0" ]] \
  && ok "historical inscription 100 resolves to the expected id" \
  || bad "historical inscription 100 does not match the pre-migration source"
[[ "$(jq -er '.inscriptions[0].genesis_height' <<<"$old")" == "4609847" ]] \
  && ok "inscription 100 genesis height 4609847 intact" \
  || bad "inscription 100 genesis height changed"

# 3. Checkpoint-adjacent state, captured from the source before it was stopped.
new=$(curl -fsS --max-time 30 "$API/api/v1/inscriptions?cursor=178908755&limit=1")
[[ "$(jq -er '.inscriptions[0].inscription_id' <<<"$new")" \
   == "2d963af95246c7a47b849da1a253d7dc407b0c0003ea30971c35648319446b9ei0" ]] \
  && ok "checkpoint-adjacent inscription 178908755 resolves to the expected id" \
  || bad "checkpoint-adjacent inscription 178908755 does not match the source"
[[ "$(jq -er '.inscriptions[0].genesis_height' <<<"$new")" == "5782326" ]] \
  && ok "inscription 178908755 genesis height 5782326 intact" \
  || bad "inscription 178908755 genesis height changed"

# 4. The inventory must not report itself as partial. This field is carried on the
#    inscriptions response, not on capabilities.
[[ "$(jq -er '.inventory_complete' <<<"$new")" == "true" ]] \
  && ok "inventory reports complete" || bad "inventory reports incomplete"

# 5. Caught up to the live chain, allowing for normal tip movement.
(( tip - height <= 2 )) && ok "index is at the Dogecoin tip (within 2 blocks)" \
                        || printf 'INFO  still catching up, %s blocks remaining\n' "$(( tip - height ))"

exit "$fail"
