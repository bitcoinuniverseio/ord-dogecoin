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

## Compute sizing and the vCPU quota

`CandidateTypes` is a tiered pool from `m7i.16xlarge` down to `m8i.2xlarge`,
ordered largest first. It is not a single size on purpose. The account's
Standard Spot quota is 32 vCPU in total and unrelated workloads routinely hold
most of it, so a pool of only 32 vCPU types has no placeable pool and the
controller waits on capacity forever. A tiered pool takes the best type that has
both capacity and quota headroom, and starts using the larger sizes on its own
if a pending quota increase is granted.

Every candidate must be x86-64. The `ord` binary is x86-64, so Graviton types
must never enter the pool.

Because instance size therefore varies between replacements, the indexer tuning
cannot be a constant. Bootstrap derives it from the hardware present on each
boot into `control/indexer.env.auto`: half of RAM as the REDB cache capped at
64 GiB, and `nproc * 2` parallel RPC requests capped at 32. The unit reads that
file before the operator-owned `control/indexer.env`, so manual overrides win.

## Measured RPC path

Dogecoin Core stays on Universe Indexers and is reached over an SSH reverse
tunnel bound to the AWS loopback interface. Measured from the AWS host while the
migration was saturating the link: 95 ms for a single `getblock`, and 261 full
blocks per second at 32 concurrent requests. The remaining catch-up is therefore
well under an hour of fetching, so the remote node is not the bottleneck and
there is no reason to place a second copy of the Dogecoin chain on AWS storage.

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
7. Run `cutover-source.sh`, installed as `/usr/local/sbin/universe-doge-ebs-cutover`.
   It performs steps 7 to 9 as one resumable unit and refuses to start while the
   preseed is still running. Each stage leaves a marker under
   `/var/lib/universe-doge-cutover`, so an interrupted run resumes rather than
   re-copying the database. Prefer `systemctl start universe-doge-ebs-cutover`
   so the work survives an SSH disconnect.
8. It gracefully stops only the primary Ord Dogecoin writer, waits for shutdown,
   proves no open database handle remains, and leaves Dogecoin Core and the
   other indexes running. It records the source checkpoint and block hash first,
   while the indexer can still answer.
9. It then runs the final copy, requiring equal size, mode, numeric owner and
   SHA-256, and writes `control/migration-manifest.json`. The source stays intact
   as the rollback copy but is disabled, so a reboot cannot silently resume a
   writer that would diverge from the migrated database.
10. Run `sudo /usr/local/sbin/doge-index-activate verify-source` on the AWS host.
    It writes an exact `HEIGHT_LIMIT` from the migration manifest, starts the
    migrated binary, requires the same capability block count and block hash,
    stops it, records `control/source-checkpoint.ok`, and only then removes the
    limit. A failed verification leaves the limit in place and the service
    stopped.
11. Run `sudo /usr/local/sbin/doge-index-activate start-catchup`. It refuses to
    start without the checkpoint evidence, `START_INDEXER`, or with a remaining
    height limit.
    Abort immediately if the observed checkpoint is zero or behind the manifest.
12. Enable the controller and both EventBridge rules. Test one controlled Spot
    replacement before relying on unattended recovery.
13. At tip, run `control/validate-index.sh`. It checks the manifest floor, a deep
    history anchor (inscription 100 at genesis height 4609847), a
    checkpoint-adjacent anchor captured from the source before it was stopped
    (178908755 at 5782326), inventory completeness, and tip agreement. Then
    restart once on the same EBS database and confirm it catches the next block.
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
