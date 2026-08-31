#!/usr/bin/env bash
set -Eeuo pipefail
# Waits for the source writer to release the REDB, then hands off to the cutover.
# Lives on the host so a disconnected agent cannot stall the migration.
DB=/data/indexers-b/ord-dogecoin/doginals.redb
log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }

for i in $(seq 1 2880); do
  if ! lsof "$DB" 2>/dev/null | tail -n +2 | grep -q .; then
    log "Source REDB has no open handle. Starting the cutover."
    sync
    systemctl start --no-block universe-doge-ebs-cutover.service
    log "Cutover service started."
    exit 0
  fi
  if (( i % 10 == 0 )); then
    holder=$(lsof -t "$DB" 2>/dev/null | head -1)
    if [[ -n "$holder" && -r /proc/$holder/io ]]; then
      log "Still held by pid $holder, written=$(awk '/^write_bytes/{print $2}' /proc/$holder/io)"
    fi
  fi
  sleep 15
done
log "Timed out waiting for the source writer to exit."
exit 1
