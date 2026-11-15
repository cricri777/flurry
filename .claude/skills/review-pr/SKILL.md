---
name: review-pr
description: Review the open pull requests of the flurry repo (or the ones named) against CONTRIBUTING.md and post one review comment per PR. Use when asked to review, check or comment on PRs; it never pushes, approves or merges.
---

# Reviewing Flurry pull requests

Review only; do not push, approve, merge or close anything unless the user asks. The rules to check live in [CONTRIBUTING.md](../../../CONTRIBUTING.md) and [AGENTS.md](../../../AGENTS.md).

## 1. Collect the PRs

```bash
gh pr list --state open --json number,title,headRefName,baseRefName
D=$(mktemp -d /tmp/flurry-review-XXXX)
for n in <numbers>; do
  gh pr view $n --json title,body -q '"# "+.title+"\n"+.body' > $D/pr$n.md
  gh pr diff $n >> $D/pr$n.md
done
```

Read every diff in full, and read the surrounding code on `main` when a change depends on it. A PR whose base is not `main` is stacked; review it against its base.

## 2. Check each PR

- **Correctness:** wrong logic, missed error or empty cases, broken callers, and interactions with the other open PRs (for example a struct field one PR adds that another PR's code does not set).
- **Invariants:** every product and security invariant in CONTRIBUTING.md, especially read-only SQL, no Snowflake access from local commands, honest completeness and synthetic data.
- **Conventions:** architecture boundaries, no `unwrap()`/`expect()` outside tests, tests for the new behavior, docs updated.
- **Scale:** loops, traversals and queries that grow badly on large accounts.
- **PR body:** the sections and the checklist the `add-feature` skill requires, and honest verification claims.

## 3. Confirm before claiming

Check out the PR in a throwaway worktree and confirm a suspected bug before reporting it; never edit the PR's own worktree.

```bash
git fetch origin <branch>
git worktree add --detach $D/wt FETCH_HEAD
cd $D/wt && cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

- Probe a suspicion with a temporary test or a run, then discard it with `git checkout -- .`.
- Follow `flurry-verification` with a throwaway `XDG_DATA_HOME`, `XDG_CACHE_HOME` and an empty `SNOWFLAKE_HOME`, so nothing reaches the user's account.
- Merge the open PRs into a detached `origin/main` in their likely order to find conflicts, then `git merge --abort`.
- Remove the worktree when done: `git worktree remove --force $D/wt`.

## 4. Comment

Post one comment per PR, in English, with numbered findings ranked by severity, each with the concrete case that fails and a suggested fix. Mark minor points as nits, and end with the merge-conflict check result.

```bash
gh pr review <n> --comment --body-file <review.md>
```

Use `--comment` (an author cannot approve their own PR). Do not post findings you could not confirm as facts; phrase them as questions. Then give the user a short table of each PR's verdict and main points.
