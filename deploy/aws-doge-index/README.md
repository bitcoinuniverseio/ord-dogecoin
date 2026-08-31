# Dogecoin index catch-up on retained EBS

This deployment moves the existing Universe Dogecoin Ord index to one encrypted
gp3 volume and uses replaceable EC2 Spot compute. The controller never creates
an index volume. It only finds the recorded volume ID, launches Spot capacity in
the fixed Availability Zone, reattaches that volume, and restores the fixed
public address.

## Safety gates

- Keep the source `doginals.redb` until tip validation, restart validation, and
  the final EBS snapshot pass.
- Never run `mkfs` from replacement bootstrap. Format the new empty volume once,
  after checking its ID and attachment state.
- Keep `LaunchEnabled=false`, the schedule disabled, and `START_INDEXER` absent
  during migration.
- `migrate-source.sh hot-copy` is only a resumable preseed. The final pass must
  run after a graceful source stop and after `lsof` reports no database handle.
- The indexer guard fails closed when the database, runtime, manifest, RPC
  cookie, owner, source size floor, or explicit start marker is missing.
- Every Fleet request sets Spot target capacity to one and On-Demand target
  capacity to zero. There is no On-Demand fallback.
- The persistent volume is outside the stack, tagged `Persistence=retain`,
  recorded in Parameter Store, and attached with `DeleteOnTermination=false`.
- The dedicated migration SSH public key and EC2 host keys live on the retained
  volume. Replacement instances restore both before the source reconnects.

## Verified source state on 2026-08-31

| Item | Value |
| --- | --- |
| Indexer | `ord-dogecoin 1.0.2` |
| Binary digest | `76e7760172e900d58811a97839d246473153edc77aa4a0cb0b01d18b66b06e76` |
| Database | `/data/indexers-b/ord-dogecoin/doginals.redb` |
| Database bytes | `1572152922112` |
| Indexed height | `5782326` |
| Dogecoin Core | `1.14.9`, synchronized, unpruned, `txindex=1` |
| Tip at inspection | `6355203` |
| Blocks behind | `572877` |
| Completion | `90.9867%` |
| Chain identity | Index hash equals Core hash at height `5782326` |

The code persists the checkpoint in `HEIGHT_TO_BLOCK_HASH`. Startup opens the
existing REDB, derives `block_count` from the last stored height, and begins at
that count. It creates a database only when the configured path does not exist.

## Capacity calculation

The file currently uses 1,464.18 GiB. A deliberately conservative projection
uses average bytes per indexed post-activation block:

```text
1,572,152,922,112 bytes / 1,172,604 blocks = 1,340,736 bytes per block
estimated remaining growth = 715.33 GiB
estimated database at tip = 2,179.51 GiB
selected gp3 capacity = 3,200 GiB
projected free space at tip = 1,020.49 GiB, or 31.89%
```

Use XFS. Start migration performance at the lowest level the staging instance
can consume. Raise gp3 toward 32,000 IOPS and 2,000 MiB/s only when the catch-up
instance supports it and CloudWatch plus OS metrics show storage pressure.

## Deployment sequence

1. Confirm the current source checkpoint, hash agreement, process, file handle,
   filesystem, owner, permissions, size, and source disk headroom.
2. Confirm current Spot placement scores, prices, offerings, vCPU quota, and EBS
   limits. Lock the volume and every replacement to one Availability Zone.
3. Create the encrypted 3,200 GiB gp3 volume with the required retention tags.
4. Attach it to a small staging Spot instance in the fixed zone, confirm it is
   new and empty, format it once as XFS, and record its UUID.
5. Deploy the controller stack with launch and schedules disabled.
6. Run the hot copy from Universe Indexers. An interruption can resume with the
   same rsync command because it uses partial in-place delta transfer.
7. Gracefully stop only the primary Ord Dogecoin writer. Wait for shutdown and
   prove no open database handle remains. Keep Dogecoin Core and other indexes
   running.
8. Record the final source capability checkpoint and block hash. Run the final
   copy and require equal size, mode, numeric owner, and SHA-256.
9. Write `control/migration-manifest.json` with the source checkpoint, block
   hash, byte count, digests, volume ID, region, zone, and filesystem UUID.
10. Start the migrated binary with a temporary height limit equal to the source
    checkpoint. Require the same capability height and block hash. Stop it.
11. Create `control/START_INDEXER`, remove the height limit, and start catch-up.
    Abort immediately if the observed checkpoint is zero or behind the manifest.
12. Enable the controller and both EventBridge rules. Test one controlled Spot
    replacement before relying on unattended recovery.
13. At tip, validate old and recent blocks, representative transactions and
    inscriptions, restart once on the same EBS database, and catch the next new
    block.
14. Create the final snapshot, measure steady-state storage demand, reduce gp3
    IOPS and throughput safely, and terminate expensive catch-up compute.

## Required tags on the retained volume

```text
Name=doge-index-persistent
Project=doge-index-catchup
Persistence=retain
DataClass=critical-index
DeleteProtection=required
SourceHost=universe-indexers
```

The volume ID must also be stored at
`/universe/doge-index/persistent-volume-id`. Replacement logic must stop with an
error if the recorded volume is absent, in another zone, or missing retention
tags.
