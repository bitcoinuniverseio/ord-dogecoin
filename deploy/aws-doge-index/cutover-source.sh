#!/usr/bin/env bash
set -Eeuo pipefail

# Consistency cutover from the live source index to the retained AWS EBS volume.
# Every stage leaves a marker, so an interrupted run resumes instead of repeating
# work and never re-copies the whole database.

DESTINATION=${DESTINATION:-ubuntu@63.184.178.36}
SSH_KEY=${SSH_KEY:-/root/.ssh/doge-ebs-migration-ed25519}
SOURCE_DB=${SOURCE_DB:-/data/indexers-b/ord-dogecoin/doginals.redb}
RELEASE_DIR=${RELEASE_DIR:-/opt/universe-ord-dogecoin/releases/4584da8c4f8e71780671a0c9674b64a1c70a378a}
VOLUME_ID=${VOLUME_ID:-vol-011690c554c2db3e9}
REMOTE_ROOT=/mnt/doge-index
STATE=/var/lib/universe-doge-cutover
SSH=(ssh -i "$SSH_KEY" -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes)
RSYNC_SSH="ssh -i $SSH_KEY -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes"

install -d -m 0750 "$STATE"

# A second operator or agent must never run this concurrently: two final copies
# writing the same destination file would corrupt the database.
exec 9>/var/lock/universe-doge-cutover.lock
if ! flock -n 9; then
  echo "Another cutover run holds the lock. Refusing to run concurrently." >&2
  exit 1
fi
log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }

# 1. The preseed must have finished. A running rsync means the delta is not final.
if systemctl is-active --quiet universe-doge-ebs-hot-copy.service; then
  log "Hot preseed is still running. Refusing to begin the consistency cutover."
  exit 1
fi

# 2. Record the checkpoint the live indexer reports while it can still answer.
if [[ ! -s "$STATE/pre-stop.json" ]]; then
  if ! systemctl is-active --quiet universe-ord-dogecoin.service; then
    log "Source indexer is already stopped and no pre-stop checkpoint exists. Refusing to guess."
    exit 1
  fi
  curl -fsS --max-time 60 http://127.0.0.1:8390/api/v1/capabilities >"$STATE/pre-stop.json.partial"
  jq -e '.block_count > 0' "$STATE/pre-stop.json.partial" >/dev/null
  mv -f "$STATE/pre-stop.json.partial" "$STATE/pre-stop.json"
fi
BLOCK_COUNT=$(jq -er '.block_count' "$STATE/pre-stop.json")
BLOCK_HASH=$(jq -er '.block_hash' "$STATE/pre-stop.json")
INDEXED_HEIGHT=$(( BLOCK_COUNT - 1 ))
log "Pre-stop checkpoint: block_count=$BLOCK_COUNT indexed_height=$INDEXED_HEIGHT"

# 3. Graceful stop. The unit sends SIGINT, never SIGKILL, and allows 30 minutes,
#    which is what lets REDB commit and close cleanly.
if systemctl is-active --quiet universe-ord-dogecoin.service; then
  log "Stopping the source indexer so REDB closes cleanly. This can take minutes."
  systemctl stop universe-ord-dogecoin.service
fi
if systemctl is-active --quiet universe-ord-dogecoin.service; then
  log "Source indexer did not stop."
  exit 1
fi

# 4. Prove no writer holds the database open, then flush.
if lsof "$SOURCE_DB" 2>/dev/null | tail -n +2 | grep -q .; then
  log "Source REDB still has an open handle after the stop."
  exit 1
fi
sync
log "Source database is closed and flushed."

# 5. Final delta plus independent sha256 verification on both sides.
if [[ ! -s "$STATE/final-copy.ok" ]]; then
  log "Starting the final delta and integrity verification."
  DESTINATION="$DESTINATION" SSH_KEY="$SSH_KEY" \
    /usr/local/sbin/universe-doge-ebs-migrate final-copy >"$STATE/final-copy.partial" 2>"$STATE/final-copy.err"
  grep -q '^sha256=' "$STATE/final-copy.partial"
  mv -f "$STATE/final-copy.partial" "$STATE/final-copy.ok"
fi
DB_SHA=$(awk -F= '/^sha256=/{print $2}' "$STATE/final-copy.ok")
DB_BYTES=$(awk -F= '/^source_size=/{print $2}' "$STATE/final-copy.ok")
log "Verified copy: bytes=$DB_BYTES sha256=$DB_SHA"

# 6. The manifest every replacement instance validates itself against before starting.
if [[ ! -s "$STATE/migration-manifest.json" ]]; then
  FS_UUID=$("${SSH[@]}" "$DESTINATION" "findmnt -n -o UUID -T $REMOTE_ROOT" | tr -d '[:space:]')
  [[ -n "$FS_UUID" ]]
  BIN_SHA=$(sha256sum "$RELEASE_DIR/ord" | awk '{print $1}')
  jq -n --arg at "$(date -u +%FT%TZ)" \
        --arg host "$(hostname)" --arg path "$SOURCE_DB" \
        --argjson indexedHeight "$INDEXED_HEIGHT" --argjson blockCount "$BLOCK_COUNT" \
        --arg blockHash "$BLOCK_HASH" \
        --argjson databaseBytes "$DB_BYTES" --arg databaseSha256 "$DB_SHA" \
        --arg binarySha256 "$BIN_SHA" \
        --arg volumeId "$VOLUME_ID" --arg filesystemUuid "$FS_UUID" \
        '{migratedAt:$at,
          source:{host:$host,path:$path,indexedHeight:$indexedHeight,blockCount:$blockCount,
                  blockHash:$blockHash,databaseBytes:$databaseBytes,databaseSha256:$databaseSha256,
                  binarySha256:$binarySha256},
          aws:{volumeId:$volumeId,filesystemUuid:$filesystemUuid,mountPoint:"/mnt/doge-index"}}' \
     >"$STATE/migration-manifest.json.partial"
  jq -e '.source.indexedHeight > 0 and (.aws.filesystemUuid|length) > 0' \
     "$STATE/migration-manifest.json.partial" >/dev/null
  mv -f "$STATE/migration-manifest.json.partial" "$STATE/migration-manifest.json"
fi
rsync -a --rsync-path='sudo rsync' -e "$RSYNC_SSH" \
  "$STATE/migration-manifest.json" "$DESTINATION:$REMOTE_ROOT/control/migration-manifest.json"
log "Migration manifest published to the persistent volume."

# 7. The source stays intact as the rollback copy, but must not silently resume on a
#    reboot: it would diverge from the migrated database and the volume is 94% full.
systemctl disable universe-ord-dogecoin.service >/dev/null 2>&1 || true
log "Source indexer stopped and disabled. Its data is untouched and remains the rollback copy."

log "Cutover complete. START_INDEXER is deliberately NOT created here."
