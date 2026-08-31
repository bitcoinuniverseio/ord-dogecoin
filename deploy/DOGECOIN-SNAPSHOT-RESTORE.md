# Dogecoin index snapshot and restore runbook

This runbook covers the Universe-operated Dogecoin Core, ord-dogecoin, and
Dogecoin TAP services. It does not use third-party blockchain data in
production.

## Safety model

- Never copy a REDB file while its owning process has it open. A normal file
  copy is not a transactionally consistent REDB backup.
- Never stop or restart an ord-dogecoin process that is indexing, repairing,
  or committing valid work.
- SQLite snapshots are created with SQLite's online backup API and verified
  with `PRAGMA quick_check` before compression. The backup copies one WAL
  snapshot in a single source read transaction so a hot writer cannot
  repeatedly invalidate page batches and starve the snapshot.
- Uploads use the B2 SHA-1 object hash for transport verification and also
  publish the required SHA-256 digest in `SHA256SUMS` and `manifest.json`.
- `latest.json` is updated only after the remote object hash matches.

## Install deterministic monitoring and TAP backups

From a clean checkout at the intended commit:

```bash
install -d -m 0750 /data/indexers-c/dogecoin-snapshots
install -m 0755 deploy/linux/universe-dogecoin-health /usr/local/sbin/
install -m 0755 deploy/linux/universe-dogecoin-snapshot /usr/local/sbin/
install -D -m 0755 deploy/linux/sqlite-online-backup.py \
  /usr/local/libexec/universe/sqlite-online-backup.py
install -m 0644 deploy/linux/universe-dogecoin-health.service /etc/systemd/system/
install -m 0644 deploy/linux/universe-dogecoin-health.timer /etc/systemd/system/
install -m 0644 deploy/linux/universe-dogecoin-snapshot.service /etc/systemd/system/
install -m 0644 deploy/linux/universe-dogecoin-snapshot.timer /etc/systemd/system/
install -m 0644 deploy/linux/universe-ord-dogecoin-full.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now universe-dogecoin-health.timer
systemctl enable --now universe-dogecoin-snapshot.timer
systemctl enable universe-ord-dogecoin-full.service
```

Enabling the full-index unit makes it survive a reboot but does not start it.
Start it only after confirming its separate database path is unused, its volume
has adequate headroom, and the primary ord-dogecoin service is still healthy.

The health result is written atomically to
`/var/lib/universe-dogecoin-health/status.json`. Historical samples are appended
to `history.jsonl`. A non-zero health unit exit is an alert signal and the exact
failure list is recorded in the JSON and journal.

The primary index is reported as stalled only when both its published height and
database modification time remain unchanged beyond `ORD_STALL_SECONDS`. This
allows long redb transactions to finish without producing a false stall alert.

## Restore TAP on a fresh server

1. Install the exact `index-doge-tap` commit from the selected manifest.
2. Download `latest.json`, its referenced manifest, `SHA256SUMS`, and the `.zst`
   object from `dogecoin/tap` in the configured indexer backup bucket.
3. Verify and decompress into a staging directory:

   ```bash
   sha256sum --check SHA256SUMS
   zstd -d tap-dogecoin-HEIGHT-TIMESTAMP.sqlite.zst -o index-doge-tap.sqlite
   python3 - <<'PY'
   import sqlite3
   db = sqlite3.connect('file:index-doge-tap.sqlite?mode=ro', uri=True)
   assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
   db.close()
   PY
   ```

4. Keep the currently running database in place. With the TAP service stopped
   on the new or standby host, install the verified file at the path required by
   its environment, correct ownership and mode, then start the service.
5. Require `/live` and `/ready` to return 200. Read authenticated `/status` and
   confirm the reader, SQLite synchronization, and source checkpoint fields.

## Restore Dogecoin Core and ord-dogecoin

Prefer a fully synchronized Universe-operated Dogecoin Core node. Do not replace
it with an older bootstrap. Confirm `initialblockdownload=false`, equal block and
header heights, and transaction lookups before starting ord-dogecoin.

For REDB, use only a snapshot whose manifest identifies the ord-dogecoin commit,
REDB version, enabled index flags, Dogecoin block height, and block hash. Verify
SHA-256 before decompression. Confirm the file is closed and stable before
moving it into place, then start the matching binary against loopback Dogecoin
Core RPC. Validate `/api/v1/capabilities`, compare its block hash with Core at
the same height, and let it index only the remaining blocks.

The current live REDB cannot be snapshotted safely without stopping its owner.
Use a blue-green pair: keep one synchronized instance serving while the other is
cleanly stopped and snapshotted. Never trade index integrity for a file-level
copy of an actively changing REDB.

The full replacement service is defined in
`deploy/linux/universe-ord-dogecoin-full.service`. It creates a separate REDB
with DRC-20, Dunes, and transaction indexing enabled. Those flags are fixed when
the database is created. Do not point that unit at a pre-existing database that
was created with a different flag set.
