#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo build -p flurry

export XDG_DATA_HOME=$(mktemp -d /tmp/flurry-verify-XXXX)
export XDG_CACHE_HOME=$XDG_DATA_HOME
trap 'rm -rf "$XDG_DATA_HOME"' EXIT
flurry=target/debug/flurry
skill=.claude/skills/flurry-verification

expect() {
  local needle=$1
  shift
  local out
  out=$("$@")
  grep -qF -- "$needle" <<<"$out" || { echo "FAIL: '$needle' missing from: $*"; echo "$out"; exit 1; }
}

python3 "$skill/seed.py" "$flurry"
expect "AB123" "$flurry" status
"$flurry" snapshots >/dev/null
for entity in users roles warehouses; do
  for format in table json csv; do
    "$flurry" "$entity" list --format "$format" >/dev/null
  done
done
expect "ANALYST" "$flurry" roles list --search analyst
expect "DATA_READ_ONLY" "$flurry" roles inherits ANALYST
expect "ANALYST" "$flurry" roles inherits DATA_READ_ONLY --inverse
expect "ACCESS: JANE USAGE on WH_ANALYTICS" "$flurry" access jane wh_analytics
expect "NO ACCESS" "$flurry" access jane wh_etl
expect "WH_ANALYTICS  ACCESS via ANALYST -> DATA_READ_ONLY" python3 "$skill/tui.py" "$flurry" 1 l G
expect "WH_ETL  NO ACCESS" python3 "$skill/tui.py" "$flurry" 1 l G

empty=$(mktemp -d /tmp/flurry-verify-XXXX)
trap 'rm -rf "$XDG_DATA_HOME" "$empty"' EXIT
printf 'default_connection_name = "prod"\n[connections.dev]\naccount = "x"\n[connections.prod]\naccount = "y"\n' >"$empty/config.toml"
tui_empty() { XDG_DATA_HOME=$empty XDG_CACHE_HOME=$empty SNOWFLAKE_HOME=$empty python3 "$skill/tui.py" "$flurry" "$@"; }
small_tui() { TUI_ROWS=8 python3 "$skill/tui.py" "$flurry" "$@"; }
expect "4/5" small_tui 2 '\x06'
expect "2/5" small_tui 2 '\x04'
expect "1/5" small_tui 2 '\x06' '\x02'
expect "3/5" small_tui 2 '\x06' '\x15'
expect "No snapshot yet: press R to sync" tui_empty
picker=$(tui_empty R)
for needle in "Sync from profile" "prod" "dev" "Esc/q cancel"; do
  grep -qF -- "$needle" <<<"$picker" || { echo "FAIL: '$needle' missing from the picker"; echo "$picker"; exit 1; }
done
! tui_empty R '\x1b' | grep -qF "Sync from profile" || { echo "FAIL: Esc did not close the picker"; exit 1; }

failed=$(tui_empty R '\r')
for needle in "Sync prod finished" "Failed: Missing 'user'" "Enter close"; do
  grep -qF -- "$needle" <<<"$failed" || { echo "FAIL: '$needle' missing from the sync window"; echo "$failed"; exit 1; }
done
! tui_empty R '\r' '\r' | grep -qF "finished" || { echo "FAIL: Enter did not close the sync window"; exit 1; }

expect "Deleted 1 snapshot(s)." "$flurry" delete 1
expect "Deleted 0 snapshot(s)." "$flurry" delete --all

echo "verify: all checks passed"
