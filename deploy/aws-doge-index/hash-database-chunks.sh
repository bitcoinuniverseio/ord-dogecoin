#!/usr/bin/env bash
set -Eeuo pipefail

# Produce a fixed-offset SHA-256 manifest for a large database. The manifest is
# replaced atomically only after every expected chunk has been hashed.

FILE=${1:?Usage: hash-database-chunks.sh FILE OUTPUT [WORKERS] [CHUNK_MIB]}
OUTPUT=${2:?Usage: hash-database-chunks.sh FILE OUTPUT [WORKERS] [CHUNK_MIB]}
WORKERS=${3:-16}
CHUNK_MIB=${4:-64}

[[ "$WORKERS" =~ ^[1-9][0-9]*$ ]]
[[ "$CHUNK_MIB" =~ ^[1-9][0-9]*$ ]]
[[ -f "$FILE" ]]

bytes=$(stat -c %s "$FILE")
(( bytes > 0 ))
chunk_bytes=$(( CHUNK_MIB * 1024 * 1024 ))
chunks=$(( (bytes + chunk_bytes - 1) / chunk_bytes ))
partial="${OUTPUT}.partial.$$"

cleanup() {
  rm -f -- "$partial"
}
trap cleanup EXIT

install -d -m 0750 "$(dirname "$OUTPUT")"
printf 'file=%s bytes=%s chunks=%s workers=%s chunk_mib=%s start=%s\n' \
  "$FILE" "$bytes" "$chunks" "$WORKERS" "$CHUNK_MIB" "$(date -u +%FT%TZ)" >&2

seq 0 $((chunks - 1)) | xargs -P "$WORKERS" -I{} sh -c '
  digest=$(dd if="$2" bs="$3"M skip="$1" count=1 status=none | sha256sum | cut -d" " -f1)
  printf "%s\t%s\n" "$1" "$digest"
' _ {} "$FILE" "$CHUNK_MIB" | sort -n >"$partial"

awk -v expected="$chunks" '
  BEGIN { ok = 1 }
  NF != 2 || $1 != NR - 1 || length($2) != 64 || $2 !~ /^[0-9a-f]+$/ { ok = 0 }
  END { if (NR != expected || !ok) exit 1 }
' "$partial"

chmod 0640 "$partial"
mv -f -- "$partial" "$OUTPUT"
trap - EXIT
printf 'done lines=%s end=%s\n' "$(wc -l <"$OUTPUT")" "$(date -u +%FT%TZ)" >&2
