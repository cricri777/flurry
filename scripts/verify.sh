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

echo "verify: all checks passed"
