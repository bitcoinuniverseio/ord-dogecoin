# Ord-Dogecoin preserved archive recovery

## Current checkpoint: 2026-10-19

Live inspection supersedes the initial launch observation below. The storage
unit is inactive (PID 0), and the indexing unit is failed (PID 0, signal 9).
The signal alone does not establish its cause. Do not restart the failed unit.
Diagnose the prior exit and review a guarded successor before any activation.

The retained delta is present at the path below: 393,216 logical bytes and
364,544 allocated bytes. Read-only QEMU 8.2.2 image information reports qcow2,
virtual size 2,258,359,144,448 bytes, and the exact preserved original as its raw
backing file. Its SHA-256 is
`6dabd4c27c8a0eee1718e7516b693d4c2110f7ce83f4c4b42c0c4d692824954f`.
The digest stayed unchanged during inspection. No FUSE view is mounted.
These are storage identity checks, not native database integrity or readiness.
Preserve the original, delta, runtime and deployment evidence together.

A separate historical disposable full copy is absent; that does not mean this
qcow2 recovery set is absent. Full-copy admission needs about 1.49 TB more space
including a 300 GB reserve. That shortfall applies to the full-copy route;
review the retained sparse-delta route before building a competing backend.
Delta growth still needs an explicit capacity budget and safe exhaustion policy.

The current archive remains unready. No original was opened with native redb,
no storage repair/commit/rebase was performed, and no service was started during
this inspection. Synthetic bounded-journal and unprivileged read-only-view
fixtures are preparation evidence only. They do not qualify the actual archive.

Recorded 2026-10-16. Both archive storage and indexing units were installed
and enabled; the deployment receipt records `startup-requested`. At 22:20:10
UTC the native indexer and QEMU storage backend were active, enabled, and had
zero restarts. Native database initialization was reading the archive; HTTP
port 8391 was not yet listening and no indexed height had been reported. The five
disposable storage/native-redb fixture cases passed before launch. Successful
archive recovery, tip readiness, and complete historical inscription coverage
remain separate live acceptance gates.

## Ownership and data path

```text
Universe Dogecoin Core on indexers-2
  -> protected existing RPC tunnel, indexers-1 loopback 22566
  -> one Ord-Dogecoin archive owner on indexers-1
  -> private HTTP listener, indexers-1 loopback 8391

preserved doginals.redb (read only)
  + persistent sparse changes.qcow2
  -> writable FUSE view doginals.redb
  -> native redb recovery and subsequent indexing
```

The restored indexers-1 host is `51.222.43.129`; the existing shared
Dogecoin Core host is new indexers-2 `51.222.108.117`. The former source
host was deleted per operator confirmation; never query it. Old host addresses and AWS migration
instructions in earlier runbooks are historical. Do not start a second Core
or another Ord-Dogecoin owner to follow those earlier layouts.

## Storage and runtime

| Item | Installed value |
| --- | --- |
| Preserved base | `/home/universe-restored/ord-dogecoin-archive/561b9bca/source/doginals.redb` |
| Base size | `2258359144448` bytes |
| Writable live view | `/var/lib/universe-ord-dogecoin-archive/doginals.redb` |
| Persistent changes | `/var/lib/universe-ord-dogecoin-archive/changes.qcow2` |
| Runtime directory | `/opt/universe-ord-dogecoin-archive/47a9e588` |
| Recovered binary | `ord-accel-v9`, reported version `1.0.2` |
| Executable SHA-256 | `47a9e588a5a102f88427377762d90e977394b96594312f64e8f6ced3662a4372` |
| Storage unit | `universe-ord-dogecoin-archive-storage.service` |
| Indexing unit | `universe-ord-dogecoin-mainnet.service` |
| Deployment evidence | `/var/lib/universe-restore-indexers1/ord-doge-live-20261016/` |
| HTTP | `127.0.0.1:8391` |
| Preserved first-inscription height | `4609723` |

The recovered executable has independent artifact identity. Its version string
or directory name does not establish that it was built from today's checkout.
Current source uses schema 6 and Cargo.lock resolves redb 2.6.3. Record the
complete executable digest in the deployment receipt, alongside the archive
identity and storage-tool versions. Do not relabel the recovered artifact as a
new source build.

## First-start safety gates

1. Verify the shared Core is Dogecoin mainnet, unpruned, beyond the preserved
   checkpoint, and out of initial block download. Verify its genesis and the
   index checkpoint hash using only the owned node. Keep existing credentials
   in their protected input files; never print them in logs or shell arguments.
2. Prove no Ord-Dogecoin process owns this archive or an alternative production
   index on current hosts involved in this recovery. Never query retired hosts. Match executable and database identity,
   rather than rejecting every process named `ord`; independent Bitcoin Signet
   work is a different owner.
3. Preserve the original archive as read-only backing storage. Do not open it
   with a native redb tool: even an apparently read-only metadata operation can
   cause native recovery writes.
4. Require real disposable redb fixtures using the pinned native library and
   the actual QEMU 8.2.2 qcow2/FUSE stack. Verify base immutability, native
   recovery, durable writes, remount persistence, and failure refusal before
   applying that stack to the large archive.
5. Validate unit files, free-space headroom, expected listeners, exact artifact
   identity, protected RPC input, and the view's backing-chain identity. Require
   the indexing unit to depend on the storage unit and refuse startup if the
   mount or identity is wrong.
6. Start the one prepared owner. Retain receipts and bounded, secret-free
   journal observations. Allow valid recovery and indexing to continue.

The launched profile uses eight RPC requests, a 4 GB native database cache,
and indexing-unit memory controls of 10 GiB high / 12 GiB maximum. The storage
backend has a 1 GiB maximum. Initial inspection uses `Restart=no`, unlimited
graceful stop time, and no forced kill. Existing RPC credentials were reused
through protected cookie input; no credential rotation was required.

A diagnostic `repair_aborted` callback indicates that a recovery attempt was
aborted by that diagnostic. It alone does not prove database corruption. Do not
clear recovery flags, patch database bytes, or substitute an empty database.

## Observed initial launch

At 22:20:10 UTC, the native process was PID 2214390 and the storage backend
was PID 2214326. The original file identity was unchanged; only the storage
backend held its base-file descriptor, independently verified read-only. The
mounted view was 2,258,359,144,448 bytes and the delta allocated 364,544 bytes.
Logs showed connection to the existing loopback RPC with protected cookie input
and a 4 GB native cache before database initialization. These are initial
observations, not successful recovery or a current indexed checkpoint.

## Observe recovery and catch-up

Use systemd state, the storage process and mount, bounded journal reads, and
HTTP responses together. Opening the database successfully is one gate;
serving from it and advancing durable checkpoints are later gates. A listener
or active process alone proves neither. Long native recovery can precede HTTP
availability. Do not terminate valid recovery or indexing to obtain a faster
health response.

When HTTP is available, inspect the actual recovered binary's response contract
before applying the current source's `/api/v1/capabilities` expectations. Compare
indexed height and block hash with an independent read of the owned Core at
that height. Record progress across at least two durable checkpoints.

Full readiness requires matching current Core height and hash, then sustained
advancement with new blocks, usable inscription reads, and no fatal recovery or
reorg state. A completed storage fixture, a requested startup, or catch-up
progress must not be reported as tip readiness.

## Historical completeness

Keeping `--first-inscription-height 4609723` preserves the recovered owner's
configuration; it does not prove that no inscriptions exist before that height
or that every inscription is present. Report historical coverage as unverified
until the boundary, archive schema/capabilities, and complete chain traversal
are established. Verify representative early and recent inscription records
against owned data. Do not turn an unknown count into an empty inventory.

## Persistence and maintenance

Keep the immutable base and its persistent delta together. The delta alone is
not a full backup. Never move, replace, truncate, compact, reset, or commit the
overlay into the only preserved base while it is in use. Before maintenance,
record base identity, backing-chain identity, checkpoint, binary digest, and
both unit states. Only an approved idle maintenance window permits a graceful
stop; never interrupt active valid recovery or indexing. Preserve a
transactionally consistent base-plus-delta recovery set, together with the
service configuration and pinned runtime, and verify a restore
before any retention decision.

Rollback retains that complete recovery set. Never use `qemu-img commit` or
`qemu-img rebase` to write changes into the preserved original. Do not discard
the delta to make startup appear successful. Diagnose storage or configuration
failures without changing the base; restore a verified separate recovery set
only after the active owner is safely idle and stopped. Keep consumer readiness
closed until the restored owner passes the same checks.

Any new consumer must use the existing owned service after readiness passes.
There is no third-party blockchain fallback and no duplicate indexer deployment.
