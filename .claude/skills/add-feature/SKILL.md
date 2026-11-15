---
name: add-feature
description: Process for adding a feature to the flurry repo: work in its own git worktree and branch, follow CONTRIBUTING.md and AGENTS.md, run the quality gates and verification, rebase on main, then open a PR. Use when asked to implement a feature, add functionality, or start a new roadmap item.
---

# Adding a feature to Flurry

Follow every step in order; the task is not done until the PR is open and its URL is reported. Do not stop after the code or the verification, and do not edit files on `main` or leave changes uncommitted (this applies to skill and docs changes too).

Follow this order. The rules live in [CONTRIBUTING.md](../../../CONTRIBUTING.md) and [AGENTS.md](../../../AGENTS.md); read them, do not restate them in the PR.

## 1. Scope it

- Read the feature in [ROADMAP.md](../../../ROADMAP.md). Work milestones in order unless the user says otherwise.
- For a large change, check with the user first (CONTRIBUTING asks for an issue or discussion).
- Check the product and security invariants in CONTRIBUTING.md. If the change could weaken one, stop and discuss it before coding.

## 2. Create the worktree and branch

Never develop on `main`. Do this before the first edit; if you already edited on `main`, move the changes into the worktree and restore `main`.

```bash
git fetch origin
git worktree add ../flurry-<slug> -b feat/<slug> origin/main
cd ../flurry-<slug>
```

Use `feat/` for features, `fix/` for bugs and `chore/` for tooling. The first `cargo build` in a new worktree recompiles everything; do not share `CARGO_TARGET_DIR` between worktrees, because another worktree's binary can silently run the wrong schema.

## 3. Implement

- Follow the architecture boundaries in CONTRIBUTING.md: keep pure logic in the right crate, and make the CLI and TUI call the same application service.
- Keep the code simple and in the project's conventions: typed `Result`, no `unwrap()`/`expect()` outside tests, no `unsafe`.
- Add unit tests next to the code in `#[cfg(test)] mod tests`, covering the error and incomplete-data cases. Add integration tests under `crates/<crate>/tests/` when the behavior crosses a boundary.
- Update docs and examples (README, `docs/usage.md`, or ROADMAP status) when the user-visible behavior changes.
- Use synthetic names only in fixtures and examples.

## 4. Run the gates

All three must pass with zero warnings before committing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

If `cargo fmt --all --check` fails, run `cargo fmt --all` and check again.

## 5. Verify the feature runs

Tests passing is not verification; run the feature itself. Follow `flurry-verification` from the worktree root:

1. Build in the worktree (`cargo build -p flurry`) and confirm `target/debug/flurry` is that build, not a stale one.
2. Export a throwaway `XDG_DATA_HOME` and `XDG_CACHE_HOME` under `/tmp`, then run `seed.py`. If it fails on the schema, update `seed.py` as part of the feature.
3. Exercise every changed CLI command and check its output against the expected behavior.
4. For any TUI change, drive the TUI with `tui.py` using the exact keys or `:` commands the feature adds, and check the final screen. Cover the normal and the error or empty case.
5. If the feature touches Snowflake connectivity or sync, also run `flurry-live-verification`; the user runs the live `sync` or `connection test` command, you do not handle their credentials.
6. Record each command and what you saw for the PR. If something cannot be verified here, say so in the PR and do not claim it works.

## 6. Commit

```bash
git add <files>
git commit -m "<imperative summary>"
```

Commit only the files for this feature. End the commit message with:

```
Co-Authored-By: <current model name> <noreply@anthropic.com>
```

## 7. Rebase, push and open the PR

```bash
git fetch origin
git rebase origin/main
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace
git push -u origin feat/<slug>
gh pr create --base main --title "<summary>" --body-file <body.md>
```

Rerun the gates after the rebase. The PR body must include:

- **Problem and design:** why the change exists and how it works.
- **Behavior changed:** user-visible effects.
- **Commands run and results:** each gate and verification command, with pass or fail.
- **Verification status:** what was verified and what was not (for example, live Snowflake checks the user must run).
- **Security and privacy:** the impact on the invariants, and the PR checklist from CONTRIBUTING.md.
- **Known limitations.**

End the PR description with:

```
🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

## 8. Clean up

After the PR is merged, remove the worktree and branch:

```bash
git worktree remove ../flurry-<slug>
git branch -d feat/<slug>
```
