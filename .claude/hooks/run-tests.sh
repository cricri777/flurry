#!/usr/bin/env bash
# Stop hook: runs the workspace tests when Rust files changed, and hands failures back to Claude.
cd "$CLAUDE_PROJECT_DIR" || exit 0
# A hook-triggered retry must not loop forever.
grep -q '"stop_hook_active": *true' && exit 0
git status --porcelain | grep -q '\.rs$' || exit 0
out=$(cargo test --workspace 2>&1) && exit 0
echo "$out" | tail -40 >&2
exit 2
