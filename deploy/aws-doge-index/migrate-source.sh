#!/usr/bin/env bash
set -Eeuo pipefail

SOURCE_DIR=${SOURCE_DIR:-/data/indexers-b/ord-dogecoin}
SOURCE_DB=${SOURCE_DB:-$SOURCE_DIR/doginals.redb}
RELEASE_DIR=${RELEASE_DIR:-/opt/universe-ord-dogecoin/releases/4584da8c4f8e71780671a0c9674b64a1c70a378a}
RPC_COOKIE=${RPC_COOKIE:-/etc/universe-dogecoin/ord-rpc.cookie}
DESTINATION=${DESTINATION:?set DESTINATION to the restricted EC2 SSH target}
SSH_KEY=${SSH_KEY:?set SSH_KEY to the dedicated migration key}
MODE=${1:-hot-copy}
REMOTE_ROOT=/mnt/doge-index
SSH=(ssh -i "$SSH_KEY" -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes)
RSYNC_SSH="ssh -i $SSH_KEY -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes"

remote() {
  "${SSH[@]}" "$DESTINATION" "sudo bash -lc $(printf '%q' "$*")"
}

database_is_open() {
  lsof "$SOURCE_DB" 2>/dev/null | tail -n +2 | grep -q .
}

copy_common() {
  remote "install -d -m 0750 $REMOTE_ROOT/data $REMOTE_ROOT/runtime $REMOTE_ROOT/control $REMOTE_ROOT/secrets"
  nice -n 10 ionice -c2 -n7 rsync -aS --numeric-ids --partial --inplace --info=progress2 --rsync-path='sudo rsync' -e "$RSYNC_SSH" \
    "$SOURCE_DB" "$DESTINATION:$REMOTE_ROOT/data/doginals.redb"
  rsync -a --numeric-ids --info=progress2 --rsync-path='sudo rsync' -e "$RSYNC_SSH" \
    "$RELEASE_DIR/ord" "$RELEASE_DIR/subsidies.json" "$RELEASE_DIR/starting_sats.json" \
    "$DESTINATION:$REMOTE_ROOT/runtime/"
  rsync -a --numeric-ids --info=progress2 --rsync-path='sudo rsync' -e "$RSYNC_SSH" \
    "$RPC_COOKIE" "$DESTINATION:$REMOTE_ROOT/secrets/ord-rpc.cookie"
  remote "chown -R 981:978 $REMOTE_ROOT/data $REMOTE_ROOT/runtime $REMOTE_ROOT/secrets && chmod 0600 $REMOTE_ROOT/data/doginals.redb $REMOTE_ROOT/secrets/ord-rpc.cookie && chmod 0755 $REMOTE_ROOT/runtime/ord"
}

if [[ "$MODE" == "hot-copy" ]]; then
  if ! database_is_open; then
    echo "Source REDB has no active writer. Use final-copy only after recording the stopped checkpoint." >&2
    exit 1
  fi
  copy_common
  echo "Hot copy completed. It is only a resumable preseed and is not authoritative."
  exit 0
fi

if [[ "$MODE" != "final-copy" ]]; then
  echo "Usage: $0 hot-copy|final-copy" >&2
  exit 2
fi

if database_is_open; then
  echo "Source REDB still has an open handle." >&2
  exit 1
fi

copy_common

source_size=$(stat -c %s "$SOURCE_DB")
source_mode=$(stat -c %a "$SOURCE_DB")
source_owner=$(stat -c '%u:%g' "$SOURCE_DB")
destination_result=$(mktemp)
trap 'rm -f "$destination_result"' EXIT
remote "stat -c '%s %a %u:%g' $REMOTE_ROOT/data/doginals.redb && sha256sum $REMOTE_ROOT/data/doginals.redb" \
  >"$destination_result" &
destination_pid=$!
source_hash=$(sha256sum "$SOURCE_DB" | awk '{print $1}')
wait "$destination_pid"
destination_summary=$(cat "$destination_result")
destination_size=$(awk 'NR==1 {print $1}' <<<"$destination_summary")
destination_mode=$(awk 'NR==1 {print $2}' <<<"$destination_summary")
destination_owner=$(awk 'NR==1 {print $3}' <<<"$destination_summary")
destination_hash=$(awk 'NR==2 {print $1}' <<<"$destination_summary")

[[ "$source_size" == "$destination_size" ]]
[[ "$source_mode" == "$destination_mode" ]]
[[ "$source_owner" == "$destination_owner" ]]
[[ "$source_hash" == "$destination_hash" ]]

printf 'source_size=%s\ndestination_size=%s\nsha256=%s\nmode=%s\nowner=%s\n' \
  "$source_size" "$destination_size" "$source_hash" "$source_mode" "$source_owner"
echo "Final consistent copy verified. Do not create START_INDEXER until checkpoint validation is complete."
