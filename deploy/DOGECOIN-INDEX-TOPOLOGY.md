# Dogecoin index topology (since 2026-09-16)

One shared Dogecoin Core, two ord indexes, three hosts. No third-party data.

```
                     universe-indexers-1 (159.195.109.76)
                     +--------------------------------------------+
                     | Dogecoin Core 1.14.9 txindex  127.0.0.1:22566 |
                     |   gap shim 22567, blockbook 19138            |
                     |   index-doge-tap 3013                        |
                     |   ord full index (serving, stale)  :8391     |
                     |   loopback :8390  -> socket-proxyd ---------+-----+
                     +----------^----------------------^-----------+     |
              ssh -L 22566      |                      | ssh -L 22566    |
   +----------------------------+--+     +-------------+-----------------+--+
   | universe-indexers-2 (152.53.92.251) |     | universe-indexers-1-new (OVH 51.222.43.129) |
   | ord-accel-v9 doginals.redb 2.26 TB  |     | ord 4584da8 index-all.redb (BUILD)          |
   |   127.0.0.1:8390 (doginals authority)|     |   drc20 + dunes + transactions, :8391       |
   |   socat peer 152.53.92.251:8390 ----+---->|   /data/indexers/data/indexers-c/...        |
   |   (range = indexers-1 only)          |     +---------------------------------------------+
   +-------------------------------------+
```

## Doginals authority (indexers-2)

| Item | Value |
| --- | --- |
| Unit | `universe-ord-dogecoin.service` + drop-in `10-shared-rpc.conf` |
| Binary | `/usr/local/bin/ord-accel-v9` via `/usr/local/bin/start_ord_dogecoin.sh` |
| Index | `/var/lib/universe-ord-0.29/ord-dogecoin/doginals.redb` |
| RPC | `universe-dogecoin-rpc-tunnel.service`: `ssh -L 127.0.0.1:22566` to indexers-1, key `/etc/universe/ord-dogecoin-rpc-tunnel-ed25519`, authorized on indexers-1 with `from="152.53.92.251",restrict,port-forwarding,permitopen="127.0.0.1:22566"` |
| Interhost | `universe-ord-dogecoin-peer.service` (socat, bind 152.53.92.251:8390, `range=159.195.109.76/32`) |
| Consumer | indexers-1 `universe-ord-dogecoin-remote.socket` on 127.0.0.1:8390 |
| Verify | `/api/v1/inscriptions?limit=1` -> `block_count`, `block_hash`, `inventory_complete:true`; anchors inscription 100 at 4609847 and 178908755 at 5782326 (`?cursor=<n>&limit=1`) |

`universe-dogecoin.service` on indexers-2 is disabled on purpose. Its chain data
was lost on 2026-09-15 when the 3.9 TB volume filled up (`No space left on
device` in `debug.log`, block index corrupted). Do not re-enable it: the shared
node on indexers-1 serves this authority, and a second Dogecoin Core would
refill the volume.

## Full index build (indexers-1-new)

The drc20/dunes/transactions index (`index-all.redb`, ord release
`4584da8c4f8e71780671a0c9674b64a1c70a378a`) could not catch up on indexers-1:
1.55 M blocks behind at roughly 270 blocks per hour, IO starved by the other
indexers on the same NVMe volume (IO full stall 60 %, 65 GB swap). The build
runs on the OVH host instead:

| Item | Value |
| --- | --- |
| Units | `universe-dogecoin-rpc-tunnel.service`, `universe-ord-dogecoin-full.service` (`deploy/linux/indexers-1-new/`) |
| Data | `/data/indexers/data/indexers-c/ord-dogecoin-full/index-all.redb` (sdb3, 3.7 TB) |
| RPC | same tunnel pattern, key `/etc/universe/ord-dogecoin-rpc-tunnel-ed25519`, authorized on indexers-1 with `from="51.222.43.129"`; indexers-1 `UNIVERSE_SSH` allowlist includes 51.222.43.129 |
| Seed | `deploy/linux/universe-ord-dogecoin-full-relocate` (runs on indexers-1 as the transient `universe-ord-dogecoin-full-relocate.service`): waits for the OVH SMART long tests, hot rsync, stops the source builder, delta rsync, SHA-256 compare, restarts source, starts destination. Log `/var/log/universe-indexers1-migration/ord-dogecoin-full-relocate.log` |

While the OVH build catches up, indexers-1 keeps serving `:8391` at its stale
height so the explorer overlay, index-doge-tap and the Hostinger tunnel keep
their endpoint. Cut-over when the OVH index reaches the node tip:

1. Compare `GET /api/v1/capabilities` on both hosts against
   `dogecoin-cli getblockcount` / `getblockhash`.
2. On indexers-1 add a peer-restricted forwarder for `:8391` to
   `51.222.43.129` (same pattern as `universe-ord-dogecoin-remote`), stop and
   disable `universe-ord-dogecoin-full.service` there, then verify the overlay
   `ord-dogecoin` source reports `ready` with `lagBlocks` 0.
3. Update `/etc/universe-dogecoin/health.env` (`ORD_URL`, `ORD_DB`) so the
   health timer measures the serving index.

## Health

`universe-dogecoin-health.timer` on indexers-1 reports the full index as the
primary ord authority (`/etc/universe-dogecoin/health.env`). It fails, honestly,
until the full index reaches tip. The doginals authority is verified by hash
against the node in the same script through loopback `:8390`.
