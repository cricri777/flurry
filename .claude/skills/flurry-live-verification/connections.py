#!/usr/bin/env python3
"""Lists Snowflake CLI connections with non-secret fields only; never prints passwords, passcodes or key contents."""
import os
import stat
import sys
import tomllib
from pathlib import Path

SAFE = ("account", "user", "authenticator", "role", "warehouse", "private_key_file", "private_key_path")


def config_dir() -> Path:
    if os.environ.get("SNOWFLAKE_HOME"):
        return Path(os.environ["SNOWFLAKE_HOME"])
    home = Path.home()
    return home / ".snowflake" if (home / ".snowflake").exists() else home / ".config/snowflake"


def load(path: Path) -> dict:
    try:
        return tomllib.loads(path.read_text())
    except FileNotFoundError:
        return {}
    except tomllib.TOMLDecodeError as e:
        sys.exit(f"{path}: invalid TOML ({e})")


d = config_dir()
config = load(d / "config.toml")
connections = load(d / "connections.toml") or config.get("connections", {})
default = os.environ.get("SNOWFLAKE_DEFAULT_CONNECTION_NAME") or config.get("default_connection_name") or "default"

print(f"config dir: {d}")
for f in ("config.toml", "connections.toml"):
    p = d / f
    if p.exists():
        mode = stat.S_IMODE(p.stat().st_mode)
        note = "" if mode & 0o077 == 0 else "  <-- readable by others, run: chmod 600 " + str(p)
        print(f"{f}: mode {mode:o}{note}")

if not connections:
    print("connections: none")
    sys.exit(0)

for name, c in connections.items():
    flag = " (default)" if name == default else ""
    fields = ", ".join(f"{k}={c[k]}" for k in SAFE if k in c)
    secrets = [k for k in ("password", "passcode", "token", "private_key_raw") if k in c]
    extra = f"  [secret keys present: {', '.join(secrets)}]" if secrets else ""
    print(f"- {name}{flag}: {fields}{extra}")
