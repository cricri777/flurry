#!/usr/bin/env python3
"""Seeds a synthetic snapshot into $XDG_DATA_HOME/flurry/flurry.db (never real data)."""
import os
import sqlite3
import subprocess
import sys

root = sys.argv[1] if len(sys.argv) > 1 else sys.exit("usage: seed.py <flurry-binary>")
data = os.environ.get("XDG_DATA_HOME") or sys.exit("XDG_DATA_HOME must point to a temp dir")
if not data.startswith(("/tmp", "/var/tmp")):
    sys.exit("refusing to seed outside /tmp")

subprocess.run([root, "status"], check=True, capture_output=True)  # creates the schema
db = sqlite3.connect(f"{data}/flurry/flurry.db")
db.execute("DELETE FROM snapshots")
for t in ("datasets", "users", "roles", "warehouses", "user_roles", "role_edges", "warehouse_privileges"):
    db.execute(f"DELETE FROM {t}")

db.execute("INSERT INTO snapshots VALUES (1, 'default', 'ACME-TEST', 'ACCOUNTADMIN', strftime('%s','now'))")
now = "strftime('%s','now')"
for name, src, latency, n in [
    ("users", "SHOW USERS", None, 2),
    ("roles", "SHOW ROLES", None, 5),
    ("warehouses", "SHOW WAREHOUSES", None, 2),
    ("user_roles", "ACCOUNT_USAGE.GRANTS_TO_USERS", 120, 3),
    ("role_edges", "ACCOUNT_USAGE.GRANTS_TO_ROLES", 120, 3),
    ("warehouse_privileges", "ACCOUNT_USAGE.GRANTS_TO_ROLES", 120, 3),
]:
    db.execute(f"INSERT INTO datasets VALUES (1, ?, ?, ?, ?, {now})", (name, src, latency, n))

db.executemany("INSERT INTO users VALUES (1, ?, ?, ?, ?, ?)", [
    ("JANE", "jane", "ANALYST", "false", "2024-01-01"),
    ("JOHN", "john", "ENGINEER", "true", "2024-02-01"),
])
db.executemany("INSERT INTO roles VALUES (1, ?, ?, ?, ?)", [
    ("ACCOUNTADMIN", None, "2023-01-01", None),
    ("SYSADMIN", "ACCOUNTADMIN", "2023-01-01", None),
    ("ANALYST", "SECURITYADMIN", "2024-01-01", "BI users"),
    ("ENGINEER", "SECURITYADMIN", "2024-01-01", None),
    ("DATA_READ_ONLY", "SECURITYADMIN", "2024-01-01", None),
])
db.executemany("INSERT INTO warehouses VALUES (1, ?, ?, ?, ?, ?, ?)", [
    ("WH_ANALYTICS", "SYSADMIN", "STANDARD", "X-Small", "2024-01-01", None),
    ("WH_ETL", "SYSADMIN", "STANDARD", "Medium", "2024-01-01", None),
])
db.executemany("INSERT INTO user_roles VALUES (1, ?, ?, ?, ?)", [
    ("JANE", "ANALYST", "SECURITYADMIN", "2024-01-02"),
    ("JOHN", "ENGINEER", "SECURITYADMIN", "2024-02-02"),
    ("JOHN", "ANALYST", "SECURITYADMIN", "2024-02-03"),
])
db.executemany("INSERT INTO role_edges VALUES (1, ?, ?, ?, ?)", [
    ("ANALYST", "DATA_READ_ONLY", "SECURITYADMIN", "2024-01-03"),
    ("ENGINEER", "DATA_READ_ONLY", "SECURITYADMIN", "2024-01-03"),
    ("SYSADMIN", "ENGINEER", "SECURITYADMIN", "2024-01-04"),
])
db.executemany("INSERT INTO warehouse_privileges VALUES (1, ?, ?, ?, ?, ?)", [
    ("DATA_READ_ONLY", "WH_ANALYTICS", "USAGE", "false", "SYSADMIN"),
    ("ENGINEER", "WH_ETL", "USAGE", "false", "SYSADMIN"),
    ("ENGINEER", "WH_ETL", "OPERATE", "false", "SYSADMIN"),
])
db.commit()
print("seeded synthetic snapshot 1")
