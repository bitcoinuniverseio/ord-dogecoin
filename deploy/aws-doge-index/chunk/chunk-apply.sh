#!/usr/bin/env bash
set -Eeuo pipefail
FILE=${1:?file}
LIST=${2:?list}
CHUNK_MB=${3:-64}
n=0
# The index list is read on fd 3, never on stdin. Redirecting the loop from the
# list would hand that list to dd as its input and write the text into the
# database instead of the streamed chunk.
while read -r idx <&3; do
  [[ "$idx" =~ ^[0-9]+$ ]] || continue
  # iflag=fullblock is required: from a pipe dd otherwise takes one short read
  # and exits, writing a fraction of the chunk and killing the sender.
  dd of="$FILE" bs="${CHUNK_MB}M" seek="$idx" count=1 \
     conv=notrunc,nocreat iflag=fullblock status=none
  n=$((n+1))
done 3< "$LIST"
printf 'applied=%s\n' "$n" >&2
