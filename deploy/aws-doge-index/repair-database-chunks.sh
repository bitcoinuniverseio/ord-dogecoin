#!/usr/bin/env bash
set -Eeuo pipefail

# Repair only fixed-offset chunks that differ between the stopped source REDB
# and its retained EBS copy. A full destination rehash and whole-file SHA-256
# comparison are required before final-copy.ok is published.

DESTINATION=${DESTINATION:-ubuntu@63.184.178.36}
SSH_KEY=${SSH_KEY:-/root/.ssh/doge-ebs-migration-ed25519}
SOURCE_DB=${SOURCE_DB:-/data/indexers-b/ord-dogecoin/doginals.redb}
REMOTE_DB=${REMOTE_DB:-/mnt/doge-index/data/doginals.redb}
STATE=${STATE:-/var/lib/universe-doge-cutover}
REMOTE_STATE=${REMOTE_STATE:-/mnt/doge-index/control}
HASH_WORKERS=${HASH_WORKERS:-16}
CHUNK_MIB=${CHUNK_MIB:-64}
SOURCE_HASHES=${SOURCE_HASHES:-$STATE/src-chunks.tsv}
DESTINATION_HASHES=${DESTINATION_HASHES:-$STATE/dst-chunks.tsv}
HASHER=${HASHER:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/hash-database-chunks.sh}
APPLIER=${APPLIER:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/chunk/chunk-apply.sh}
REMOTE_HASHER=/usr/local/sbin/doge-hash-database-chunks
REMOTE_APPLIER=/usr/local/sbin/doge-apply-database-chunks
REMOTE_HASHES=$REMOTE_STATE/dst-chunks.tsv
REMOTE_DIFFERENCES=$REMOTE_STATE/differing-chunks.txt
SSH=(ssh -i "$SSH_KEY" -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes)

[[ "$HASH_WORKERS" =~ ^[1-9][0-9]*$ ]]
[[ "$CHUNK_MIB" =~ ^[1-9][0-9]*$ ]]
[[ -x "$HASHER" ]]
[[ -f "$APPLIER" ]]
[[ -f "$SOURCE_DB" ]]

install -d -m 0750 "$STATE"
exec 8>/var/lock/universe-doge-ebs-destination.lock
if ! flock -n 8; then
  echo "Another transfer already holds the destination lock." >&2
  exit 1
fi

remote() {
  "${SSH[@]}" "$DESTINATION" "sudo bash -lc $(printf '%q' "$*")"
}

if lsof "$SOURCE_DB" 2>/dev/null | tail -n +2 | grep -q .; then
  echo "Source REDB still has an open handle." >&2
  exit 1
fi
if remote "systemctl is-active --quiet universe-ord-dogecoin.service"; then
  echo "Destination indexer is active." >&2
  exit 1
fi
if remote "lsof '$REMOTE_DB' 2>/dev/null | tail -n +2 | grep -q ."; then
  echo "Destination REDB still has an open handle." >&2
  exit 1
fi

source_bytes=$(stat -c %s "$SOURCE_DB")
destination_bytes=$(remote "stat -c %s '$REMOTE_DB'")
[[ "$source_bytes" == "$destination_bytes" ]]
chunk_bytes=$(( CHUNK_MIB * 1024 * 1024 ))
chunks=$(( (source_bytes + chunk_bytes - 1) / chunk_bytes ))

validate_manifest() {
  local manifest=$1
  awk -v expected="$chunks" '
    BEGIN { ok = 1 }
    NF != 2 || $1 != NR - 1 || length($2) != 64 || $2 !~ /^[0-9a-f]+$/ { ok = 0 }
    END { if (NR != expected || !ok) exit 1 }
  ' "$manifest"
}

validate_manifest "$SOURCE_HASHES"
remote "cat '$REMOTE_HASHES'" >"${DESTINATION_HASHES}.partial"
validate_manifest "${DESTINATION_HASHES}.partial"
mv -f -- "${DESTINATION_HASHES}.partial" "$DESTINATION_HASHES"

differences="$STATE/differing-chunks.tsv"
indexes="$STATE/differing-chunks.txt"
awk 'NR == FNR { destination[$1] = $2; next }
     !($1 in destination) || destination[$1] != $2 { print $1 "\t" $2 }' \
  "$DESTINATION_HASHES" "$SOURCE_HASHES" >"${differences}.partial"
mv -f -- "${differences}.partial" "$differences"
cut -f1 "$differences" >"${indexes}.partial"
mv -f -- "${indexes}.partial" "$indexes"
difference_count=$(wc -l <"$differences")
printf 'differing_chunks=%s total_chunks=%s\n' "$difference_count" "$chunks"

# Install the exact versioned hasher on the target before it is used for the
# post-repair proof.
"${SSH[@]}" "$DESTINATION" "sudo install -m 0755 /dev/stdin '$REMOTE_HASHER'" <"$HASHER"
"${SSH[@]}" "$DESTINATION" "sudo install -m 0755 /dev/stdin '$REMOTE_APPLIER'" <"$APPLIER"
"${SSH[@]}" "$DESTINATION" "sudo tee '$REMOTE_DIFFERENCES' >/dev/null" <"$indexes"

if (( difference_count > 0 )); then
  while read -r index <&3; do
    dd if="$SOURCE_DB" bs="${CHUNK_MIB}M" skip="$index" count=1 status=none
  done 3<"$indexes" | \
    "${SSH[@]}" "$DESTINATION" \
      "sudo '$REMOTE_APPLIER' '$REMOTE_DB' '$REMOTE_DIFFERENCES' '$CHUNK_MIB'" \
      >"$STATE/repaired-chunks.log" 2>&1
  remote "sync"
fi

remote "'$REMOTE_HASHER' '$REMOTE_DB' '$REMOTE_HASHES' '$HASH_WORKERS' '$CHUNK_MIB'"
remote "cat '$REMOTE_HASHES'" >"${DESTINATION_HASHES}.verified"
validate_manifest "${DESTINATION_HASHES}.verified"
cmp -s "$SOURCE_HASHES" "${DESTINATION_HASHES}.verified"
mv -f -- "${DESTINATION_HASHES}.verified" "$DESTINATION_HASHES"

destination_result=$(mktemp)
trap 'rm -f "$destination_result"' EXIT
remote "stat -c '%s %a %u:%g' '$REMOTE_DB' && sha256sum '$REMOTE_DB'" >"$destination_result" &
destination_pid=$!
source_hash=$(sha256sum "$SOURCE_DB" | awk '{print $1}')
wait "$destination_pid"
destination_summary=$(cat "$destination_result")
destination_size=$(awk 'NR == 1 {print $1}' <<<"$destination_summary")
destination_mode=$(awk 'NR == 1 {print $2}' <<<"$destination_summary")
destination_owner=$(awk 'NR == 1 {print $3}' <<<"$destination_summary")
destination_hash=$(awk 'NR == 2 {print $1}' <<<"$destination_summary")
source_mode=$(stat -c %a "$SOURCE_DB")
source_owner=$(stat -c '%u:%g' "$SOURCE_DB")

[[ "$source_bytes" == "$destination_size" ]]
[[ "$source_mode" == "$destination_mode" ]]
[[ "$source_owner" == "$destination_owner" ]]
[[ "$source_hash" == "$destination_hash" ]]

partial="$STATE/final-copy.partial"
printf 'source_size=%s\ndestination_size=%s\nsha256=%s\nmode=%s\nowner=%s\n' \
  "$source_bytes" "$destination_size" "$source_hash" "$source_mode" "$source_owner" >"$partial"
mv -f -- "$partial" "$STATE/final-copy.ok"
printf 'Final fixed-offset repair verified: bytes=%s sha256=%s\n' "$source_bytes" "$source_hash"
