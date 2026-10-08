"""Read-only aggregate audit. Never emits records or creates database copies."""
import hashlib
import json
import pathlib
import sqlite3
import sys

path = pathlib.Path(sys.argv[1]).resolve(strict=True)
with sqlite3.connect(path.as_uri() + "?mode=ro", uri=True) as database:
    database.execute("PRAGMA query_only=ON")
    tables = {}
    for name in ("games", "aliases", "save_paths", "launch_history", "settings"):
        rows = sorted(
            json.dumps(row, ensure_ascii=False, separators=(",", ":"))
            for row in database.execute(f"SELECT * FROM {name}")
        )
        checksum = hashlib.sha256()
        for row in rows:
            checksum.update(row.encode("utf-8"))
            checksum.update(b"\n")
        tables[name] = {"count": len(rows), "checksum": checksum.hexdigest()}
    print(json.dumps({
        "schema": database.execute("PRAGMA user_version").fetchone()[0],
        "integrity": database.execute("PRAGMA integrity_check").fetchone()[0],
        "foreign_key_violations": len(database.execute("PRAGMA foreign_key_check").fetchall()),
        "tables": tables,
    }))
