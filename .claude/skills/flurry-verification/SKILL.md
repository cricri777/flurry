---
name: flurry-verification
description: Build, test and drive the flurry CLI/TUI in this repo. Use when asked to run flurry, try a command, check a change works, or reproduce a bug; covers offline testing with a synthetic snapshot and what must be left to the user (live Snowflake logins).
---

# Driving flurry

Flurry is a Rust workspace; the app is the `flurry` crate. Run everything from the repo root.

## Rules

- Never ask for, read, print or store a Snowflake password, MFA code or token; do not `cat` `~/.config/snowflake/` files.
- `sync` and `connection test` talk to a real account and need a live login (password and MFA code, or a key): tell the user to run them with the `!` prefix, then read the local results with `status` and `list`.
- For your own tests use a throwaway data directory so the user's real snapshot is never touched.

## Check the code

```bash
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

## Test offline with a synthetic snapshot

```bash
export XDG_DATA_HOME=$(mktemp -d /tmp/flurry-XXXX) XDG_CACHE_HOME=$(mktemp -d /tmp/flurry-XXXX)
cargo build -p flurry
python3 .claude/skills/flurry-verification/seed.py target/debug/flurry   # users, roles, warehouses, grants
target/debug/flurry status
target/debug/flurry roles list --search read
target/debug/flurry users list --format json
```

- The seed has 2 users, 5 roles, 2 warehouses and a small role hierarchy (`ANALYST` and `ENGINEER` inherit `DATA_READ_ONLY`, which has `USAGE` on `WH_ANALYTICS`).
- To test a new table or query, extend `seed.py` so the fixture stays synthetic; never copy real account metadata into the repo.
- The seed refuses to run unless `XDG_DATA_HOME` is under `/tmp`.

## TUI

Open the TUI and send keys in a pseudo-terminal, with no tmux. Each argument is one key sequence; the script prints the final screen and quits:

```bash
python3 .claude/skills/flurry-verification/tui.py target/debug/flurry            # start screen
python3 .claude/skills/flurry-verification/tui.py target/debug/flurry j j        # move down twice
```

Keys: `j`/`k` move, `Tab` and `1`-`3` switch tabs, `l`/`h` focus detail or list, `/` search, `:` command, `Esc` normal mode, `q` quit. Use the same exported `XDG_*` variables as above.

## Test config handling

Point `SNOWFLAKE_HOME` at a temp directory with a hand-written `config.toml` (fake values only) and run `flurry connection test`; it fails at the network step, which is enough to check that the connection resolves. The resolution rules themselves have unit tests in `crates/flurry/src/config.rs`.

## Where things live

| File | Role |
|---|---|
| `crates/flurry/src/main.rs` | CLI, output formats and the TUI loop |
| `crates/flurry/src/config.rs` | Snowflake CLI style config loading |
| `crates/flurry/src/session.rs` | Logins, session cache, read-only guard and queries |
| `crates/flurry/src/collect.rs` | Snowflake queries and normalization into a snapshot |
| `crates/flurry/src/store.rs` | SQLite schema, atomic snapshot save and local queries |
| `crates/flurry/src/modes.rs` | Vim-style modal key handling |
