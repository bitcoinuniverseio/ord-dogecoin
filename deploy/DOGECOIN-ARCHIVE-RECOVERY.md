# Ord-Dogecoin preserved archive recovery

## Current checkpoint: 2026-10-19

Live inspection supersedes the initial launch observation below. The storage
unit is inactive (PID 0), and the indexing unit is failed (PID 0, signal 9).
The signal alone does not establish its cause. The 2026-10-19 bounded audit
review found a privileged `systemctl kill` command with an explicit kill-signal
flag for this unit immediately before the recorded signal-9 exit, followed by
a stop command. The requesting actor and motivation remain unverified; this
is administrative-intervention evidence, not native database corruption proof.
Do not restart the failed unit. Review a guarded successor before activation.

The retained v2 fixture receipt records all five storage/native cases passed:
unaligned IO/truncate/zero-gap/sync; missing-mount refusal; daemon-loss persistence
with base unchanged; native recovery/reopen with process-lock exclusion; and
native recovery persistence after a process crash with unchanged base hash.
The fixture never opened the original archive. Its native scanner digest is
`6711b8f9649a543900a2f8639d147beda9f6f2683a1596b32a1a2381f31df779`.
These completed fixtures were inspected, not rerun. The legacy storage graph
marks the base file/raw nodes read-only and the overlay file/qcow2 nodes writable.
Production startup still needs exact live identity, exclusive ownership, current
owned RPC, capacity exhaustion handling and review of the retained startup guards.

The retained delta is present at the path below: 393,216 logical bytes and
364,544 allocated bytes. Read-only QEMU 8.2.2 image information reports qcow2,
virtual size 2,258,359,144,448 bytes, and the exact preserved original as its raw
backing file. Its SHA-256 is
`6dabd4c27c8a0eee1718e7516b693d4c2110f7ce83f4c4b42c0c4d692824954f`.
The digest stayed unchanged during inspection. No FUSE view is mounted.
A later read-only `qemu-img check -f qcow2 --output=json` completed with exit 0
and `check-errors=0` in 0.830 seconds. No repair flag was used. Delta SHA-256 and
base metadata remained unchanged. The reported 34,459,826 virtual clusters are
an image-layout denominator, not database scan progress; only one data cluster
is allocated. Never rerun this completed check merely for a fresh timestamp.
These are storage identity/metadata checks, not native database integrity or readiness.
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

## Successor preflight module

`deploy/archive_recovery_admission.py` provides a preflight policy and a Linux
owner lease. It contains no service launcher. Callers must use privately captured
observations, a protected expected manifest, and the same held lease throughout
any separately reviewed successor lifetime. A passed snapshot is not native health.
Do not unlink or replace the lease path while an owner holds it.

Preflight refuses stale or future-dated observations, changed source/delta/runtime
identity, a live legacy owner or mounted view, absent read-only-source proof,
unverified capacity exhaustion handling, and stale/unauthenticated/unowned RPC.
Expected source/delta metadata includes device, inode, size, timestamps, ownership,
mode and link count. The manifest must reserve at least 300 GB, plus all remaining
growth up to its explicit maximum delta allocation. This is a budget calculation,
not enforcement of a disk limit. No recovery is admitted until exhaustion handling
has independent proof. The module does not grant that proof itself.

Policy tests: `python -m unittest discover -s deploy -p test_archive_recovery_admission.py -v`.
On Windows, eight policy cases passed and two Linux-only lease cases were skipped.
Those two lease cases then passed on I1 Linux in a fresh network-isolated synthetic
fixture: contention preserves bytes and refuses a second owner; symlinks, hardlinks
and a writable parent are rejected. No production lease, native database or service
was opened by those tests. Do not rerun the completed remote fixture for freshness.

A live collector on 2026-10-19 acquired
`/var/lib/universe-ord-dogecoin-archive/recovery-successor-owner.lock` around its
snapshot, verified the exact original identity, retained delta and executable
digests, and reused the completed qcow metadata-check receipt. The executable
is 307,059,112 bytes; a 256 MiB collector bound first refused it. A corrected
collector required that exact size and used a 320 MiB streaming-read bound with
idle IO priority. The first refusal and its evidence remain preserved.

The collector matched the approved SSH forward to an established TCP socket
on new indexers-2 and verified both authenticated RPC routes agreed at 6,421,264
blocks/headers, IBD false. Source/delta/runtime identities and unit/route ownership
were unchanged across collection. The lease was released afterwards; that
snapshot is not continuing ownership or future fresh RPC evidence.

Preflight correctly refused exactly `source_readonly_verified` and
`growth_exhaustion_verified`. A live enforced read-only source view and verified
exhaustion handling are still missing. Its illustrative 600 GB maximum delta
budget plus 300 GB reserve fits the observed 1.068 TB free space, but arithmetic
is not enforcement or a prediction of recovery write volume. Runtime lifetime
lease integration and the guarded successor launcher remain incomplete. The
failed legacy unit must stay stopped; this collector does not authorize a restart.

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


### 19 October actual source read-only mount qualification

At Unix 1792387588.935756 the exact retained 2,258,359,144,448-byte source
was bind-mounted in a private mount and network namespace under the existing
cooperating-owner lease. The view matched the original device, inode and size.
Mount flags were ro,nosuid,nodev,noexec; both root write-only and read-write
opens returned EROFS. Only a read-only descriptor was opened: no archive content
read, truncation, write, unlink or native database open was attempted. Source
and retained delta metadata remained unchanged, both legacy owners remained
PID0, and no global mount survived namespace exit. The lease was then released.

Receipt: /home/universe-restore/doge-actual-ro-view-v1-20261019.json. This
completed one-time proof must not be rerun. It establishes actual source-view
enforcement for that namespace, not a persistent runtime view, privileged
external-writer exclusion or production ownership. Keep admission closed until
the successor integrates this view and the lease for its entire lifetime, and
independently verifies enforced growth limits and safe exhaustion behavior.
Capacity arithmetic alone cannot satisfy that gate. No service was activated.


### 19 October bounded QEMU file-growth qualification

A new disposable 16 MiB raw base and qcow2 overlay were tested with the installed
QEMU 8.2.2. The writer inherited a hard 2 MiB RLIMIT_FSIZE and ignored SIGXFSZ
so the storage error could propagate. After flushing a 512 KiB pattern, a
separate 4 MiB write returned EFBIG. The image stayed within 2,097,152 bytes
(2,035,712 allocated bytes). Read-only reopening verified the flushed prefix;
qemu-img check reported zero check errors and zero corruptions. The synthetic
base digest was unchanged. No original archive, retained delta or native
database was opened, and no service changed.

Receipt: /home/universe-restore/doge-qcow-growth-limit-v1-20261019.json; completed
once, do not rerun. This qualifies storage refusal and retained flushed data
for this small EFBIG case. It does not prove physical ENOSPC, native database
error propagation/recovery, QEMU storage-daemon/FUSE lifetime integration or a
production size limit. A per-process file-size ceiling limits each file, not
total filesystem consumption, and does not reserve capacity against other
writers. Keep growth_exhaustion_verified false until the native owner and
retained-stack failure path are qualified and the actual limit is enforced for
the writer lifetime. Do not infer readiness from this synthetic result.

References: [Linux resource limits](https://www.man7.org/linux/man-pages/man2/getrlimit.2.html)
and [QEMU image utility](https://www.qemu.org/docs/master/tools/qemu-img.html).


### 19 October native recovery after bounded QEMU growth failure

A new tiny dirty synthetic redb database was created with the retained pinned
scanner (SHA-256 6711b8f9649a543900a2f8639d147beda9f6f2683a1596b32a1a2381f31df779).
QEMU exported its qcow2 overlay through FUSE in a private mount/network namespace.
With the daemon file-size ceiling fixed at the initial 196,616-byte image size,
native recovery returned exit 1 with an I/O error and the delta did not grow.
After orderly storage shutdown, qemu-img check reported zero errors. The same
delta, reopened with a 16 MiB synthetic ceiling, completed recovery (exit 2, the
scanner's inspection-required recovery result); its next open returned exit 0
with clean integrity. The synthetic base digest stayed unchanged. No original
archive, retained production delta or production service was touched.

The harness initially sent QMP quit and disconnected without awaiting its
response, leaving an idle synthetic daemon waiting. Only the local SSH observer
was closed. Exact fixture identities and completed native logs were checked;
acknowledged QMP quit commands then completed the three already-planned phases
without native signals, replay or duplicate workers. Future shutdown code must
keep the QMP connection open until the quit response is received and then verify
the daemon exit and FUSE mount removal. Do not reuse the initial disconnect-only
shutdown sequence.

Receipts: /home/universe-restore/doge-native-growth-v1-20261019.json and
/home/universe-restore/doge-native-growth-qmp-completion-v1-20261019.json.
This completed synthetic scenario must not be rerun. It qualifies the pinned
scanner's recovery through this FUSE file-limit failure, not physical ENOSPC,
application writes or large-archive recovery. Production admission stays closed
until lifetime source-view/lease/limit enforcement, dedicated worker identity,
capacity reserve policy and native application readiness are integrated and
reviewed. Preserve the original archive and its existing delta throughout.


### Reusable acknowledged QMP shutdown transport

`deploy/archive_recovery_qmp.py` now provides `acknowledged_quit` for a fresh
connected Linux Unix socket. It validates the expected peer PID via SO_PEERCRED,
requires a trusted idle-owner callback before negotiation and again immediately
before quit, and matches command IDs while awaiting the quit response. One
monotonic deadline bounds the exchange; response buffers and asynchronous events
are bounded. EOF, peer errors, malformed responses, mismatched IDs or timeout
produce fixed refusal labels without exposing peer payloads. The socket timeout
is restored, and the caller retains ownership of the connection.

This helper is transport only. Its idle callback is caller policy, not proof
created by the helper. A reviewed lifecycle caller must retain its lease, qualify
the daemon identity and native-client exit, then verify process exit and mount
removal after acknowledgment. An unresolved exchange must not trigger automatic
retry or signals. The helper does not start services, send process signals or
open databases; a successful response does not establish native readiness.

Focused tests cover event interleaving, missing/error replies, changed idle
ownership, wrong IDs, deadlines, response bounds, event floods and actual Linux
peer identity. Windows protocol tests passed 8 cases with 2 Linux-only cases
skipped; initial missing-AF_UNIX handling was corrected to a guarded refusal.
I1 passed all 10 cases and one exact-PID, no-data QEMU daemon exchange: quit was
acknowledged and the daemon exited 0, with no block graph, export or database.
The one-time Linux receipt is /home/universe-restore/doge-qmp-helper-v1-20261019.json.
Do not rerun that completed fixture. Production lifecycle integration is pending.


### Cooperating-owner child lifetime primitive

`deploy/archive_recovery_lifetime.py` provides a single-supervisor `LeaseGroup`.
It acquires the guarded `OwnerLease`, tracks one storage and one native child,
and passes the same lock descriptor across child exec. This preserves the
cooperating-owner lock after a supervisor exit while a child retains that
descriptor. Role reuse is refused. Wait deadlines preserve the child. There is
no automatic context-manager cleanup, signal, terminate, kill or service action.

Storage quit requires a tracked live storage child and every tracked native
child to have exited; the QMP transport repeats that guard before issuing quit.
Lease release requires all tracked children exited and the exact mountpoint
absent from current mountinfo. The lock file is preserved. Callers must retain
the group on errors and observe existing children rather than duplicate them.

The API is a lifetime primitive, not a production admission or launch command.
Before supplying production arguments, the reviewed caller still must qualify
the executable, storage graph, private namespace, read-only source, dedicated
worker identity, enforced capacity policy, current owned RPC and external-owner
exclusion. Each actual executable must preserve the inherited descriptor; a
child that closes it cannot provide supervisor-exit lock protection. The
production archive executable has not yet passed that qualification.

Three Linux tests passed: active child blocks lease release and wait timeout
preserves it; active native blocks QMP shutdown before transport; supervisor
exit leaves the lock held until its synthetic child exits. A real empty QEMU
daemon inherited the same lease descriptor, blocked a second owner, acknowledged
QMP quit and exited 0 before release. The lock was then reacquired successfully.
No block graph, export or database was created. Completed one-time receipt:
/home/universe-restore/doge-lifetime-v1-20261019.json; do not rerun. This does
not establish a production runtime lifetime or archive readiness.


### Actual executable Testnet startup boundary

The retained 307,059,112-byte executable was freshly digest-qualified as
47a9e588a5a102f88427377762d90e977394b96594312f64e8f6ced3662a4372, with
unchanged metadata. In a private mount and network namespace its executable
and sidecars were exposed through read-only bind views. It ran with explicit
Testnet arguments, isolated data/config/cookie paths and a 16 MiB database
cache, as UID 65534 with no capabilities and no-new-privileges. That UID is a
fixture identity, not an approved production service account.

The native initialized only a new 3,686,400-byte synthetic database. At its first
getblockcount request, the local synthetic RPC observer verified the exact
executable device/inode, the inherited owner-lease descriptor, the unprivileged
process state, refusal of active-child lease release and refusal of a second
cooperating owner. The RPC returned a controlled fixture error; native exit 1
was expected. Lease release followed natural child exit, and the namespace
exited 0 without signals or service changes. No original archive, retained
production delta or production RPC was opened.

Completed receipt: /home/universe-restore/doge-native-boundary-v1-20261019.json.
Do not rerun this one-time scenario. This verifies actual executable startup
through its first RPC request, not descriptor retention through sustained
indexing/recovery, actual-archive compatibility or usable API readiness. Keep
production admission closed until the reviewed lifetime launch guard binds the
read-only source, capacity policy, dedicated identity and fresh owned RPC to
the preserved recovery set.


### Deterministic storage launch plan

`deploy/archive_recovery_storage_plan.py` builds exact argv for prlimit, env,
setpriv and QEMU storage-daemon. The hard and soft file-size limits are equal;
SIGXFSZ is ignored so file-growth errors propagate through the storage layer.
The exec chain removes CAP_SYS_RESOURCE from the capability bounding set, clears
inheritable/ambient capabilities and enables no-new-privileges. It does not
remove every capability needed by the root storage daemon to mount its FUSE
export; this is not a claim of an unprivileged storage process.

The graph explicitly opens source-file/raw read-only and delta-file/qcow2
read-write, with the exact source backing node and no automatic read-only
fallback. Block definitions use JSON. Paths must be distinct absolute normalized
Unix paths and cannot inject comma-separated QEMU options. The caller must
still verify real inode identities, parent safety, private source mount and
export paths; lexical checks alone cannot reject filesystem aliases.

The builder caps permitted per-file limits at 600 GB; that is a construction
ceiling, not an approved production budget or a filesystem reservation. Admission
must separately prove current free space, reserve and actual enforced limit.
Exports default to allow-other=off. Explicit worker-group access is described
below; dedicated production account and restored-set integration remain pending.

Three focused tests passed on Windows and Linux. In a new private mount/network
namespace, the exact generated command mounted a synthetic source-backed FUSE
view with a 1 MiB hard limit. Proc metadata confirmed read-only source and
read-write delta descriptors, the actual soft/hard limits, absent SYS_RESOURCE
capability, no-new-privileges and ignored SIGXFSZ. View write/sync/read passed;
source digest stayed unchanged and qcow metadata checks reported zero errors.
Acknowledged QMP quit produced exit 0 and mount removal before lease release.
No original archive, retained production delta or native database was opened.
Completed receipt: /home/universe-restore/doge-storage-plan-v1-20261019.json;
do not rerun. Production launch/admission and unprivileged export access remain
pending.


### Group-restricted private export access

The storage builder now accepts an explicit worker_gid. It enables allow-other
only after verifying a root-owned 0710 export gateway assigned to that group,
a root:root 0700 control directory, root-owned safe ancestors, and source, delta
and QMP paths directly inside control. Root-owned sticky temporary ancestors
are allowed for isolated fixtures. The default remains owner-only.

QEMU 8.2.2 initializes writable FUSE export attributes as root:root 0600, even
with allow-other enabled. The first worker probe was therefore denied before
any native scanner ran. Its failed receipt is preserved. `prepare_worker_export`
now requires an idle-native callback and an exact live FUSE mount, verifies the
opened descriptor's device against that same namespace mount instance, then
sets only the FUSE view's virtual attributes to root:worker-group 0660. Missing
mounts and active native work refuse permission changes. Source/delta/control
file permissions are not changed. Caller ownership, exact daemon identity and
namespace qualification are still required throughout.

The corrected continuation reused the original synthetic daemon and delta; no
fixture replay or duplicate storage worker was launched. The UID/GID 65534
worker ran with zero capabilities and no-new-privileges, was denied all four
source/delta/QMP/lock control opens, but recovered the synthetic database through
FUSE (exit 2) and reopened cleanly (exit 0). UID 65533 could not open the export.
Qcow checks reported zero errors and corruptions. The source was protected by
the private read-only mount; the before/after digest pair was not retained across
the failed observer, so no digest-comparison claim is made for this scenario.

Completion initially attempted lease reacquisition after mount removal but
before the daemon closed its last lease descriptor. That race receipt is also
preserved. A final observer verified the daemon gone, reacquired the lease and
completed the remaining metadata check without repeating native recovery.
Always verify actual child exit as well as mount removal before releasing or
reacquiring ownership. The normal tracked LeaseGroup wait-before-release order
already enforces this requirement.

Final receipt: /home/universe-restore/doge-private-export-completion-v1-20261019.json.
Six focused tests passed on Windows and Linux, including refusal before any
permission change when the native is active or the exact FUSE mount is absent.
All completed fixture and continuation stages must not be rerun. No original
archive, retained production delta, production service or production account
was changed. UID/GID 65534 is a fixture identity; a dedicated production account
and complete restored-set admission remain pending.

Reference: [QEMU 8.2.2 FUSE implementation](https://raw.githubusercontent.com/qemu/qemu/v8.2.2/block/export/fuse.c).


### Dedicated I1 archive worker identity

The service identity universe-doge-archive now exists on I1 as UID 981, GID 977.
It was created only after exact-host checks confirmed the account/group absent
and the legacy archive unit stopped. The account has /usr/sbin/nologin, a locked
authentication field, no home directory and no provisioned password or SSH key.
Its primary group is exclusive, and it has no supplementary group memberships.
All pre-existing passwd, shadow, group and gshadow records were verified
unchanged. No native worker, legacy unit or database was started.

The initial preparation script stopped at a missing sys import before account
inspection or mutation; its corrected version repeated fresh absence guards
and performed creation once. Do not rerun either creation script. Receipt:
/home/universe-restore/doge-dedicated-worker-identity-v1-20261019.json.

`deploy/archive_recovery_identity.py` now pins this identity and I1 machine ID.
Its read-only inspect_identity audit exposes only qualification flags, never a
shadow value. assess_identity rejects changed host/name/UID/GID, shared groups,
supplementary memberships, login-capable shell, home creation, enabled password
authentication or an old/future/nonfinite audit. worker_argv reaudits locally
before constructing setpriv with cleared groups, capabilities and ambient
privileges, plus no-new-privileges. It does not grant production admission.

Five focused tests passed on Windows and Linux. The generated argv also ran
a real no-data process as UID 981/GID 977, groups empty, capabilities zero and
no-new-privileges enabled. The identity was reaudited before and after. One-time
audit/probe receipt: /home/universe-restore/doge-identity-audit-v1-20261019.json;
do not rerun the completed fixture. Future production preparation may use the
read-only audit, not account creation.

Bind the dedicated group into a new private export gateway and qualify the
complete preserved recovery set, fresh owned RPC, hard growth budget and lifetime
owner before native activation. The existing original/delta ownership and failed
legacy unit are unchanged; production integration and archive readiness remain
pending. This identity is I1-specific and must not be assumed valid on new2.

### Combined preserved-set preparation decision, 19 October 2026

`deploy/archive_recovery_preparation.py` composes the existing snapshot preflight
and dedicated identity audit. The storage worker group must be exactly 977. The
planned hard file ceiling must equal the maximum used in reserve arithmetic,
fit the current delta length, and stay within the storage builder's bounds.
Reviewed capacity policy, enforced reserve and external-writer exclusion are
separate required observations. Synthetic success cannot substitute for them.
The result qualifies preparation only and always leaves production admission,
native activation and native health false. It never launches or opens storage.

Five focused composition tests passed on Windows and Linux. A fresh I1 audit
held the existing owner lease while checking the preserved source/delta metadata,
delta/runtime digests, inactive legacy owners, dedicated identity and owned RPC.
Both routes agreed at height 6,421,407 with IBD false; the tunnel's actual
established TCP peer was new2. Source and delta metadata stayed unchanged; no
source content was read, native database opened or production service changed.

The combined decision correctly refused five missing runtime proofs:
source_readonly_verified, growth_exhaustion_verified, capacity_policy_reviewed,
reserve_enforcement_verified and external_writer_exclusion_verified. Previously
completed private namespace and synthetic growth fixtures remain valid within
their stated scope; those namespaces ended and do not satisfy a live successor.
Receipt: /home/universe-restore/doge-preparation-audit-v1-20261019.json. The lease
was released after this audit. Do not rerun the completed audit as a fixture.

Next implement the reviewed storage supervisor that establishes and maintains
the private source view, hard growth limit, free-space protection and owner
exclusion for the entire lifetime. Recollect fresh evidence under its lease;
qualify the dedicated group's mounted export and actual native lifetime before
opening the preserved database. Do not restart the failed legacy unit.
