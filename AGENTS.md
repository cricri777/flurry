# Feature workflow

- Work each feature in its own git worktree and branch.
- Write code following the project's conventions.
- Add or update unit tests for the feature.
- Keep docs free of duplication: state each fact in one place and link to it from the others.
- These gates must pass: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`.
- Run the `flurry-verification` skill to execute the feature and confirm it works.
- If the feature touches Snowflake connectivity or sync, also run `flurry-live-verification`; the user runs the live command.
- If a feature cannot be verified, say so in the PR and do not claim it works.
- Rebase on `main` before opening the PR.
- Commit, push and open a GitHub PR listing the commands run and their results.
