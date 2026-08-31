#!/usr/bin/env bash
set -Eeuo pipefail

exec > >(tee -a /var/log/doge-index-bootstrap.log | logger -t doge-index-bootstrap) 2>&1

VOLUME_ID="__VOLUME_ID__"
REGION="__REGION__"
MOUNT_POINT="/mnt/doge-index"
DEVICE_NAME="/dev/sdf"

retry() {
  local attempts=$1
  local delay=$2
  shift 2
  local n=1
  until "$@"; do
    if (( n >= attempts )); then
      return 1
    fi
    sleep "$delay"
    n=$((n + 1))
  done
}

install_required_packages() {
  if command -v mkfs.xfs >/dev/null && command -v rsync >/dev/null \
      && command -v jq >/dev/null && command -v curl >/dev/null \
      && command -v iostat >/dev/null; then
    return
  fi
  export DEBIAN_FRONTEND=noninteractive
  retry 8 15 apt-get update
  retry 8 15 apt-get install -y --no-install-recommends xfsprogs rsync jq curl ca-certificates sysstat
}

find_volume_device() {
  local expected_serial
  expected_serial=${VOLUME_ID//-/}
  local device
  for device in /dev/nvme*n1; do
    [[ -b "$device" ]] || continue
    if [[ "$(lsblk -ndo SERIAL "$device" 2>/dev/null | tr -d '[:space:]')" == "$expected_serial" ]]; then
      printf '%s\n' "$device"
      return 0
    fi
  done
  if [[ -b "$DEVICE_NAME" ]]; then
    printf '%s\n' "$DEVICE_NAME"
    return 0
  fi
  return 1
}

install_required_packages

device=""
for _ in $(seq 1 180); do
  if device=$(find_volume_device); then
    break
  fi
  sleep 5
done

if [[ -z "$device" || ! -b "$device" ]]; then
  echo "Persistent volume $VOLUME_ID did not appear. Refusing to use root storage." >&2
  exit 1
fi

filesystem=$(blkid -o value -s TYPE "$device" || true)
if [[ "$filesystem" != "xfs" ]]; then
  echo "Persistent volume $VOLUME_ID is not a preformatted XFS filesystem. Refusing automatic formatting." >&2
  exit 1
fi

uuid=$(blkid -o value -s UUID "$device")
install -d -m 0750 "$MOUNT_POINT"
fstab_line="UUID=$uuid $MOUNT_POINT xfs defaults,noatime,inode64,logbufs=8,nofail,x-systemd.device-timeout=10min 0 2"
grep -qF "UUID=$uuid " /etc/fstab || printf '%s\n' "$fstab_line" >> /etc/fstab
mountpoint -q "$MOUNT_POINT" || mount "$MOUNT_POINT"

if [[ "$(findmnt -n -o SOURCE -T "$MOUNT_POINT")" == *"$(findmnt -n -o SOURCE -T /)"* ]]; then
  echo "Persistent mount resolved to root storage. Refusing to continue." >&2
  exit 1
fi

token=$(curl -fsS -X PUT -H 'X-aws-ec2-metadata-token-ttl-seconds: 300' \
  http://169.254.169.254/latest/api/token 2>/dev/null || true)
instance_id="unknown"
instance_type="unknown"
if [[ -n "$token" ]]; then
  instance_id=$(curl -fsS -H "X-aws-ec2-metadata-token: $token" \
    http://169.254.169.254/latest/meta-data/instance-id 2>/dev/null || printf 'unknown')
  instance_type=$(curl -fsS -H "X-aws-ec2-metadata-token: $token" \
    http://169.254.169.254/latest/meta-data/instance-type 2>/dev/null || printf 'unknown')
fi
history="$MOUNT_POINT/control/recovery-history.jsonl"
if ! grep -qF "\"instanceId\":\"$instance_id\"" "$history" 2>/dev/null; then
  jq -cn --arg at "$(date -u +%FT%TZ)" --arg instanceId "$instance_id" \
    --arg instanceType "$instance_type" --arg volumeId "$VOLUME_ID" \
    '{at:$at,instanceId:$instanceId,instanceType:$instanceType,volumeId:$volumeId}' >>"$history"
fi

install -d -m 0700 "$MOUNT_POINT/control/ssh"
if [[ -s "$MOUNT_POINT/control/ssh/ssh_host_ed25519_key" ]]; then
  for stored_key in "$MOUNT_POINT"/control/ssh/ssh_host_*; do
    [[ -f "$stored_key" ]] || continue
    install -o root -g root -m "$(stat -c %a "$stored_key")" "$stored_key" "/etc/ssh/$(basename "$stored_key")"
  done
  systemctl restart ssh.service
else
  for host_key in /etc/ssh/ssh_host_*; do
    [[ -f "$host_key" ]] || continue
    install -o root -g root -m "$(stat -c %a "$host_key")" "$host_key" "$MOUNT_POINT/control/ssh/$(basename "$host_key")"
  done
fi

if ! getent group universe-ord-dogecoin >/dev/null; then
  groupadd --system --gid 978 universe-ord-dogecoin
fi
if ! id universe-ord-dogecoin >/dev/null 2>&1; then
  useradd --system --uid 981 --gid 978 --home-dir /var/lib/universe-ord-dogecoin \
    --shell /usr/sbin/nologin universe-ord-dogecoin
fi

chown root:universe-ord-dogecoin "$MOUNT_POINT/control"
chmod 0750 "$MOUNT_POINT/control"
for control_file in migration-manifest.json START_INDEXER HEIGHT_LIMIT source-checkpoint.ok; do
  if [[ -e "$MOUNT_POINT/control/$control_file" ]]; then
    chown root:universe-ord-dogecoin "$MOUNT_POINT/control/$control_file"
    chmod 0640 "$MOUNT_POINT/control/$control_file"
  fi
done

if [[ -s "$MOUNT_POINT/control/ssh/authorized_keys" ]] && id ubuntu >/dev/null 2>&1; then
  install -d -o ubuntu -g ubuntu -m 0700 /home/ubuntu/.ssh
  install -o ubuntu -g ubuntu -m 0600 \
    "$MOUNT_POINT/control/ssh/authorized_keys" /home/ubuntu/.ssh/authorized_keys
fi

install -d -o universe-ord-dogecoin -g universe-ord-dogecoin -m 0750 \
  /var/lib/universe-ord-dogecoin /etc/universe-dogecoin

cat >/usr/local/sbin/doge-index-start-guard <<'GUARD'
#!/usr/bin/env bash
set -Eeuo pipefail
mountpoint -q /mnt/doge-index
test -f /mnt/doge-index/control/START_INDEXER
test -s /mnt/doge-index/control/migration-manifest.json
test -x /mnt/doge-index/runtime/ord
test -s /mnt/doge-index/runtime/subsidies.json
test -s /mnt/doge-index/runtime/starting_sats.json
test -s /mnt/doge-index/secrets/ord-rpc.cookie
test -s /mnt/doge-index/data/doginals.redb
minimum=$(jq -er '.source.databaseBytes' /mnt/doge-index/control/migration-manifest.json)
actual=$(stat -c %s /mnt/doge-index/data/doginals.redb)
(( actual >= minimum ))
expected_volume=$(jq -er '.aws.volumeId' /mnt/doge-index/control/migration-manifest.json)
[[ "$expected_volume" == "__VOLUME_ID__" ]]
device=$(findmnt -n -o SOURCE -T /mnt/doge-index)
serial=$(lsblk -ndo SERIAL "$device" | tr -d '[:space:]-')
[[ "$serial" == "${expected_volume//-/}" ]]
expected_uuid=$(jq -er '.aws.filesystemUuid' /mnt/doge-index/control/migration-manifest.json)
[[ "$(findmnt -n -o UUID -T /mnt/doge-index)" == "$expected_uuid" ]]
expected_binary=$(jq -er '.source.binarySha256' /mnt/doge-index/control/migration-manifest.json)
[[ "$(sha256sum /mnt/doge-index/runtime/ord | awk '{print $1}')" == "$expected_binary" ]]
owner=$(stat -c '%U:%G' /mnt/doge-index/data/doginals.redb)
[[ "$owner" == "universe-ord-dogecoin:universe-ord-dogecoin" ]]
if [[ -e /mnt/doge-index/control/HEIGHT_LIMIT ]]; then
  height_limit=$(tr -d '[:space:]' </mnt/doge-index/control/HEIGHT_LIMIT)
  [[ "$height_limit" =~ ^[1-9][0-9]*$ ]]
  expected_limit=$(jq -er '.source.blockCount' /mnt/doge-index/control/migration-manifest.json)
  [[ "$height_limit" == "$expected_limit" ]]
fi
GUARD
chmod 0755 /usr/local/sbin/doge-index-start-guard

cat >/usr/local/sbin/doge-index-run <<'RUNNER'
#!/usr/bin/env bash
set -Eeuo pipefail
root=/mnt/doge-index
args=(
  "--rpc-url=http://127.0.0.1:22566"
  "--cookie-file=$root/secrets/ord-rpc.cookie"
  "--index=$root/data/doginals.redb"
  "--first-inscription-height=4609723"
  "--db-cache-size=${DB_CACHE_SIZE}"
  "--nr-parallel-requests=${RPC_PARALLEL_REQUESTS}"
)
if [[ -e "$root/control/HEIGHT_LIMIT" ]]; then
  height_limit=$(tr -d '[:space:]' <"$root/control/HEIGHT_LIMIT")
  [[ "$height_limit" =~ ^[1-9][0-9]*$ ]]
  args+=("--height-limit=$height_limit")
fi
exec "$root/runtime/ord" "${args[@]}" server --http --address=127.0.0.1 --http-port=8390
RUNNER
chmod 0755 /usr/local/sbin/doge-index-run

cat >/usr/local/sbin/doge-index-activate <<'ACTIVATE'
#!/usr/bin/env bash
set -Eeuo pipefail
root=/mnt/doge-index
manifest=$root/control/migration-manifest.json
height_limit=$root/control/HEIGHT_LIMIT
start_marker=$root/control/START_INDEXER
evidence=$root/control/source-checkpoint.ok
service=doge-ord-indexer.service

install_control_value() {
  local value=$1
  local destination=$2
  local temporary="$destination.$$"
  printf '%s\n' "$value" >"$temporary"
  chown root:universe-ord-dogecoin "$temporary"
  chmod 0640 "$temporary"
  mv -f "$temporary" "$destination"
}

case "${1:-}" in
  verify-source)
    test -s "$manifest"
    expected_count=$(jq -er '.source.blockCount' "$manifest")
    expected_hash=$(jq -er '.source.blockHash' "$manifest")
    install_control_value "$expected_count" "$height_limit"
    install_control_value ready "$start_marker"
    rm -f "$evidence"
    cleanup() { systemctl stop "$service" >/dev/null 2>&1 || true; }
    trap cleanup EXIT
    systemctl start "$service"
    observed=""
    for _ in $(seq 1 180); do
      observed=$(curl -fsS --max-time 10 http://127.0.0.1:8390/api/v1/capabilities 2>/dev/null || true)
      if jq -e '.block_count > 0 and (.block_hash | length) > 0' <<<"$observed" >/dev/null 2>&1; then
        break
      fi
      sleep 5
    done
    observed_count=$(jq -er '.block_count' <<<"$observed")
    observed_hash=$(jq -er '.block_hash' <<<"$observed")
    [[ "$observed_count" == "$expected_count" ]]
    [[ "$observed_hash" == "$expected_hash" ]]
    systemctl stop "$service"
    jq -cn --arg at "$(date -u +%FT%TZ)" --argjson blockCount "$observed_count" \
      --arg blockHash "$observed_hash" '{verifiedAt:$at,blockCount:$blockCount,blockHash:$blockHash}' \
      >"$evidence.$$"
    chown root:universe-ord-dogecoin "$evidence.$$"
    chmod 0640 "$evidence.$$"
    mv -f "$evidence.$$" "$evidence"
    rm -f "$height_limit"
    trap - EXIT
    echo "Migrated source checkpoint verified: block_count=$observed_count block_hash=$observed_hash"
    ;;
  start-catchup)
    test -s "$evidence"
    test ! -e "$height_limit"
    test -s "$start_marker"
    systemctl enable --now "$service"
    ;;
  *)
    echo "Usage: $0 verify-source|start-catchup" >&2
    exit 2
    ;;
esac
ACTIVATE
chmod 0755 /usr/local/sbin/doge-index-activate

cat >/usr/local/sbin/doge-index-health <<'HEALTH'
#!/usr/bin/env bash
set -Eeuo pipefail
root=/mnt/doge-index
manifest=$root/control/migration-manifest.json
log=$root/control/index-health.jsonl
latest=$root/control/latest-health.json
test -s "$manifest"
minimum=$(jq -er '.source.indexedHeight' "$manifest")
capabilities=$(curl -fsS --max-time 10 http://127.0.0.1:8390/api/v1/capabilities) || exit 0
height=$(jq -er '.block_count - 1' <<<"$capabilities")
auth=$(cat "$root/secrets/ord-rpc.cookie")
tip_response=$(curl -fsS --max-time 10 --user "$auth" \
  --data-binary '{"jsonrpc":"1.0","id":"health","method":"getblockcount","params":[]}' \
  -H 'content-type: text/plain' http://127.0.0.1:22566/)
tip=$(jq -er '.result' <<<"$tip_response")
if (( height < minimum )); then
  printf 'expected at least %s, observed %s at %s\n' "$minimum" "$height" "$(date -u +%FT%TZ)" \
    >"$root/control/CHECKPOINT_REGRESSION"
  logger -t doge-index-health "Checkpoint regression: expected at least $minimum, observed $height"
  systemctl stop doge-ord-indexer.service
  exit 1
fi
memory_used_pct=$(awk '/MemTotal:/ {total=$2} /MemAvailable:/ {available=$2} END {printf "%.2f", (total-available)*100/total}' /proc/meminfo)
disk_used_pct=$(df --output=pcent "$root" | tail -1 | tr -dc '0-9')
database_bytes=$(stat -c %s "$root/data/doginals.redb")
record=$(jq -cn --arg at "$(date -u +%FT%TZ)" --argjson indexedHeight "$height" \
  --argjson tip "$tip" --argjson databaseBytes "$database_bytes" \
  --argjson memoryUsedPercent "$memory_used_pct" --argjson diskUsedPercent "$disk_used_pct" \
  '{at:$at,indexedHeight:$indexedHeight,tip:$tip,blocksRemaining:($tip-$indexedHeight),databaseBytes:$databaseBytes,memoryUsedPercent:$memoryUsedPercent,diskUsedPercent:$diskUsedPercent}')
printf '%s\n' "$record" >>"$log"
temporary="$latest.$$"
printf '%s\n' "$record" >"$temporary"
mv -f "$temporary" "$latest"
HEALTH
chmod 0755 /usr/local/sbin/doge-index-health

# Replacement Spot instances are deliberately not one fixed size, so the indexer
# tuning is recomputed from the hardware this boot actually has. Operator values
# in indexer.env are read afterwards and win.
total_kb=$(awk '/MemTotal:/ {print $2}' /proc/meminfo)
cache_bytes=$(( total_kb * 1024 / 2 ))
(( cache_bytes > 68719476736 )) && cache_bytes=68719476736
(( cache_bytes < 4294967296 )) && cache_bytes=4294967296
parallel=$(( $(nproc) * 2 ))
(( parallel > 32 )) && parallel=32
(( parallel < 4 )) && parallel=4
printf 'DB_CACHE_SIZE=%s\nRPC_PARALLEL_REQUESTS=%s\n' "$cache_bytes" "$parallel" \
  >"$MOUNT_POINT/control/indexer.env.auto"
chmod 0640 "$MOUNT_POINT/control/indexer.env.auto"

cat >/etc/systemd/system/doge-ord-indexer.service <<'UNIT'
[Unit]
Description=Universe Dogecoin Ord catch-up on persistent EBS
After=network-online.target mnt-doge\x2dindex.mount
Wants=network-online.target
RequiresMountsFor=/mnt/doge-index
ConditionPathExists=/mnt/doge-index/control/START_INDEXER

[Service]
Type=simple
User=universe-ord-dogecoin
Group=universe-ord-dogecoin
WorkingDirectory=/mnt/doge-index/runtime
Environment=RUST_LOG=info
Environment=SUBSIDIES_PATH=/mnt/doge-index/runtime/subsidies.json
Environment=STARTING_SATS_PATH=/mnt/doge-index/runtime/starting_sats.json
EnvironmentFile=-/mnt/doge-index/control/indexer.env.auto
EnvironmentFile=-/mnt/doge-index/control/indexer.env
ExecStartPre=/usr/local/sbin/doge-index-start-guard
ExecStart=/usr/local/sbin/doge-index-run
Restart=on-failure
RestartSec=15
KillSignal=SIGINT
TimeoutStartSec=15min
TimeoutStopSec=30min
SendSIGKILL=no
LimitNOFILE=262144
Nice=-5
OOMScoreAdjust=-500
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/mnt/doge-index /var/lib/universe-ord-dogecoin
ReadOnlyPaths=/etc/universe-dogecoin

[Install]
WantedBy=multi-user.target
UNIT

cat >/usr/local/sbin/doge-spot-interruption-watch <<'WATCH'
#!/usr/bin/env bash
set -Eeuo pipefail
token=""
while true; do
  token=$(curl -fsS -X PUT -H 'X-aws-ec2-metadata-token-ttl-seconds: 21600' \
    http://169.254.169.254/latest/api/token 2>/dev/null || true)
  if [[ -n "$token" ]] && curl -fsS -H "X-aws-ec2-metadata-token: $token" \
      http://169.254.169.254/latest/meta-data/spot/instance-action >/run/doge-spot-action.json 2>/dev/null; then
    logger -t doge-spot-watch "Spot interruption detected. Stopping indexer and flushing EBS."
    systemctl stop doge-ord-indexer.service || true
    sync
    umount /mnt/doge-index || true
    exit 0
  fi
  sleep 5
done
WATCH
chmod 0755 /usr/local/sbin/doge-spot-interruption-watch

cat >/etc/systemd/system/doge-index-health.service <<'UNIT'
[Unit]
Description=Persist Dogecoin index catch-up health
After=doge-ord-indexer.service
RequiresMountsFor=/mnt/doge-index

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/doge-index-health
UNIT

cat >/etc/systemd/system/doge-index-health.timer <<'UNIT'
[Unit]
Description=Sample Dogecoin index catch-up health every minute

[Timer]
OnBootSec=90s
OnUnitActiveSec=60s
AccuracySec=5s
Persistent=true

[Install]
WantedBy=timers.target
UNIT

cat >/etc/systemd/system/doge-spot-interruption-watch.service <<'UNIT'
[Unit]
Description=Graceful Dogecoin index shutdown on Spot interruption
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/sbin/doge-spot-interruption-watch
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable --now doge-spot-interruption-watch.service
systemctl enable --now doge-index-health.timer
if [[ -f "$MOUNT_POINT/control/START_INDEXER" ]]; then
  systemctl enable --now doge-ord-indexer.service
fi

echo "Doge index bootstrap completed for $VOLUME_ID in $REGION"
