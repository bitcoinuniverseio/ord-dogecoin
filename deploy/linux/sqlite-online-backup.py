#!/usr/bin/env python3
import sqlite3
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: sqlite-online-backup.py SOURCE DESTINATION", file=sys.stderr)
        return 2

    source = Path(sys.argv[1]).resolve(strict=True)
    destination = Path(sys.argv[2]).resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        print(f"destination already exists: {destination}", file=sys.stderr)
        return 2

    source_uri = f"file:{source.as_posix()}?mode=ro"
    with sqlite3.connect(source_uri, uri=True, timeout=60) as source_db:
        with sqlite3.connect(destination, timeout=60) as destination_db:
            source_db.backup(destination_db, pages=4096, sleep=0.05)
            result = destination_db.execute("PRAGMA quick_check").fetchone()
            if result != ("ok",):
                print(f"backup verification failed: {result!r}", file=sys.stderr)
                return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
