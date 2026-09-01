#!/usr/bin/env bash
set -Eeuo pipefail
# Hash a large file in fixed chunks, in parallel.
# rsync's delta is unusable here: with --inplace against its own basis it
# rewrites every block serially at queue depth 1. Chunk hashing is parallel,
# sequential per worker, and tells us exactly which ranges differ.
FILE=${1:?file}
OUT=${2:?out}
WORKERS=${3:-16}
CHUNK_MB=64
SIZE=$(stat -c %s "$FILE")
N=$(( (SIZE + CHUNK_MB*1048576 - 1) / (CHUNK_MB*1048576) ))
printf 'file=%s size=%s chunks=%s workers=%s start=%s\n' \
  "$FILE" "$SIZE" "$N" "$WORKERS" "$(date -u +%FT%TZ)" >&2
seq 0 $((N-1)) | xargs -P "$WORKERS" -I{} sh -c '
  h=$(dd if="$2" bs="$3"M skip="$1" count=1 2>/dev/null | sha256sum | cut -d" " -f1)
  printf "%s\t%s\n" "$1" "$h"
' _ {} "$FILE" "$CHUNK_MB" | sort -n > "$OUT"
printf 'done lines=%s end=%s\n' "$(wc -l < "$OUT")" "$(date -u +%FT%TZ)" >&2
