#!/usr/bin/env bash
set -Eeuo pipefail
STATE=/var/lib/universe-doge-cutover
SRC=/data/indexers-b/ord-dogecoin/doginals.redb
REMOTE=/mnt/doge-index/data/doginals.redb
DEST=${DESTINATION:-ubuntu@63.184.178.36}
KEY=${SSH_KEY:-/root/.ssh/doge-ebs-migration-ed25519}
CHUNK_MB=64
SSH=(ssh -i "$KEY" -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes)
log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }

# Recompute the diff positionally: both digest lists share an identical index
# ordering, so line N maps to line N. This does not depend on join's collation.
paste "$STATE/src-chunks.tsv" "$STATE/dst-chunks.tsv" \
  | awk -F'\t' '$1!=$3 {print "INDEX MISALIGNMENT" > "/dev/stderr"; exit 1} $2!=$4 {print $1}' \
  | sort -n > "$STATE/diff-sorted.txt"
n=$(wc -l < "$STATE/diff-sorted.txt")
log "chunks to repair: $n"
(( n > 0 )) || { log "nothing to repair"; exit 0; }

log "shipping index list"
"${SSH[@]}" "$DEST" "cat > /tmp/doge-diff.txt" < "$STATE/diff-sorted.txt"

log "streaming $n chunks over one connection"
while read -r idx; do
  dd if="$SRC" bs="${CHUNK_MB}M" skip="$idx" count=1 status=none
done < "$STATE/diff-sorted.txt" \
  | "${SSH[@]}" "$DEST" "sudo /usr/local/sbin/doge-chunk-apply $REMOTE /tmp/doge-diff.txt $CHUNK_MB"

log "re-verifying the repaired chunks on both sides"
/usr/local/sbin/doge-chunk-verify "$SRC" "$STATE/diff-sorted.txt" "$CHUNK_MB" 8 > "$STATE/repaired-src.tsv"
"${SSH[@]}" "$DEST" "sudo /usr/local/sbin/doge-chunk-verify $REMOTE /tmp/doge-diff.txt $CHUNK_MB 8" > "$STATE/repaired-dst.tsv"

sl=$(wc -l < "$STATE/repaired-src.tsv"); dl=$(wc -l < "$STATE/repaired-dst.tsv")
log "verified lines src=$sl dst=$dl expected=$n"
[[ "$sl" == "$n" && "$dl" == "$n" ]]
bad=$(paste "$STATE/repaired-src.tsv" "$STATE/repaired-dst.tsv" | awk -F'\t' '$2!=$4' | wc -l)
log "chunks still differing after repair: $bad"
(( bad == 0 ))
log "all $n repaired chunks now match"
