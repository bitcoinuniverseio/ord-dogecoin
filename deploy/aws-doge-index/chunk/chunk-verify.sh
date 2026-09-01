#!/usr/bin/env bash
set -Eeuo pipefail
FILE=${1:?file}
LIST=${2:?list}
CHUNK_MB=${3:-64}
WORKERS=${4:-8}
[[ -f "$FILE" ]]
[[ -s "$LIST" ]]
[[ "$CHUNK_MB" =~ ^[1-9][0-9]*$ ]]
[[ "$WORKERS" =~ ^[1-9][0-9]*$ ]]
xargs -a "$LIST" -P "$WORKERS" -I{} bash -c '
  [[ "$1" =~ ^[0-9]+$ ]] || exit 1
  h=$(dd if="$2" bs="$3"M skip="$1" count=1 2>/dev/null | sha256sum | cut -d" " -f1)
  printf "%s\t%s\n" "$1" "$h"
' _ {} "$FILE" "$CHUNK_MB" | sort -n
