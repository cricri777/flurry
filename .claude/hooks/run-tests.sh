#!/usr/bin/env bash
# Stop hook: runs the workspace tests when Rust files differ from main (committed or not), and hands failures back to Claude.
cd "$(git rev-parse --show-toplevel)" || exit 0
# A hook-triggered retry must not loop forever.
grep -q '"stop_hook_active": *true' && exit 0
base=$(git merge-base HEAD origin/main 2>/dev/null || echo HEAD)
{ git diff --name-only "$base"; git ls-files --others --exclude-standard; } | grep -q '\.rs$' || exit 0
out=$(cargo test --workspace 2>&1) && exit 0
echo "Tests failed, fix them before finishing:" >&2
echo "$out" | tail -40 >&2
exit 2
