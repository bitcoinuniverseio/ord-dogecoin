#!/usr/bin/env bash
set -Eeuo pipefail
FILE=${1:?file}
LIST=${2:?list}
CHUNK_MB=${3:-64}
[[ -f "$FILE" ]]
[[ -s "$LIST" ]]
[[ "$CHUNK_MB" =~ ^[1-9][0-9]*$ ]]
bytes=$(stat -c %s "$FILE")
chunk_bytes=$(( CHUNK_MB * 1024 * 1024 ))
chunks=$(( (bytes + chunk_bytes - 1) / chunk_bytes ))
expected=$(wc -l <"$LIST")
n=0
previous=-1
# The index list is read on fd 3, never on stdin. Redirecting the loop from the
# list would hand that list to dd as its input and write the text into the
# database instead of the streamed chunk.
while read -r idx <&3; do
  [[ "$idx" =~ ^[0-9]+$ ]]
  (( idx < chunks ))
  (( idx > previous ))
  # iflag=fullblock is required: from a pipe dd otherwise takes one short read
  # and exits, writing a fraction of the chunk and killing the sender.
  dd of="$FILE" bs="${CHUNK_MB}M" seek="$idx" count=1 \
     conv=notrunc,nocreat iflag=fullblock status=none
  n=$((n+1))
  previous=$idx
done 3< "$LIST"
[[ "$n" == "$expected" ]]
printf 'applied=%s\n' "$n" >&2
