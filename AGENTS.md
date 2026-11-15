# Feature workflow

- Work each feature in its own git worktree.
- Add unit tests for the feature.
- After each feature, run the `flurry-verification` skill to confirm it works.
- If the feature touches Snowflake connectivity or sync, also run `flurry-live-verification`.
- Commit, push and open a GitHub PR for each feature.
