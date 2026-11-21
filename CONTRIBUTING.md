# Contributing to Flurry

Flurry is an early-stage, local-first Snowflake RBAC explorer built in Rust. Contributions should improve correctness, usability, security or performance without compromising its read-only, privacy-first principles.

## Before you start

- Read the [README](README.md) for scope and Snowflake semantics, and the [Roadmap](ROADMAP.md) before a substantial feature.
- Open an issue or discussion first for a large change.
- Never use real employer, customer or production Snowflake metadata in issues, pull requests, screenshots, tests or examples.

## Development setup

```bash
rustup component add rustfmt clippy   # only if missing; install rustup from https://rustup.rs
git clone <repo-url> flurry && cd flurry
cargo build --workspace              # first build takes a couple of minutes
cargo run -p flurry -- <args>        # see docs/getting-started.md
```

Quality gates, all of which must pass before a commit:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

New to Rust? Read [The Rust Book](https://doc.rust-lang.org/book/) chapters 1-10, use `cargo check` while iterating, and add a dependency (`cargo add`) only when std can't do the job.

## Architecture boundaries

Each responsibility gets its own crate under `crates/`; italic crates are planned.

- `flurry-graph`: pure graph construction and traversal; no I/O, storage or Snowflake imports.
- *`flurry-domain`*: typed entities and RBAC semantics.
- *`flurry-snowflake`*: driver adapter, SQL, identifier quoting, result normalization.
- *`flurry-storage`*: SQLite migrations and repositories.
- *`flurry-sync`*: collection plans, progress, freshness and atomic snapshots.
- *`flurry-cli`* / *`flurry-tui`*: output and UI; they call shared services and never query Snowflake directly.
- `flurry`: the current CLI, Snowflake session, collector and SQLite store.

## Invariants

Every contribution must preserve these; discuss first if a change could weaken one.

1. **Read-only:** no `GRANT`, `REVOKE`, `CREATE`, `ALTER`, `DROP` or other mutation.
2. **Explicit network access:** only sync or targeted refresh may contact Snowflake.
3. **No telemetry:** send no metadata, queries or diagnostics externally.
4. **No secrets at rest:** never store passwords, keys or tokens in SQLite or exports.
5. **Least privilege:** do not require `ACCOUNTADMIN`; handle restricted visibility gracefully.
6. **Honest completeness:** an empty result is not proof of no access when collection is incomplete.
7. **Provenance:** keep source, timestamps, latency, row counts, warnings and per-dataset status.
8. **Synthetic fixtures only:** no real names, grants or account identifiers.
9. **Safe SQL:** parameterize values, quote identifiers with the dedicated function, never concatenate input.
10. **Local protection:** treat the database and exports as sensitive.

## Vim-friendly TUI

Vim-style navigation is a supported interaction model and is enabled by default.

- Keep arrow keys, `Tab` and `Shift+Tab` working, and keep everything possible without a mouse.
- Support Normal, Insert, Visual and command-line modes, and show the mode and shortcuts in the status line.
- Normal mode: `j`/`k` move, `h`/`l` switch panes or collapse/expand, `gg`/`G` jump, `Ctrl-d`/`Ctrl-u` page, `/` search, `n`/`N` next/previous match, `:` command line, `?` help, `Esc` back or dismiss.
- `r` refreshes from the local cache only; `R` explicitly starts a sync.
- `q` goes back or quits only when no input, modal or child view is active; `:q` and `:quit` always quit.
- Never intercept navigation keys while an input has focus.
- Put key handling in the shared navigation layer, not in one-off screen handlers, so mappings can become configurable.

## Coding guidelines

- Prefer small modules and pure functions for normalization and graph operations.
- Use typed errors and `Result`; avoid `unwrap()`/`expect()` outside tests and keep `unsafe` out except the documented SQLite FFI boundary.
- Do not swallow errors, and redact credentials and tokens from logs and errors.
- Never log raw metadata rows; user and role names are sensitive.
- Keep raw Snowflake field names at the connector boundary and use domain names internally.
- Keep output ordering deterministic, and make stale, delayed or incomplete data explicit.
- Cite the official Snowflake documentation for any semantic assumption.

## Testing

Add or update tests for every behavior change: unit tests in a `#[cfg(test)] mod tests` next to the code, integration tests in the crate's `tests/`. Priorities and the required scenarios are in the [Roadmap](ROADMAP.md#testing-strategy).

- Integration tests need a dedicated non-production Snowflake account and explicit opt-in such as `FLURRY_INTEGRATION_TESTS=1`.
- Fixtures are synthetic and fictional, including large ones (about 10k users and roles, 100k+ edges) for performance.

## Pull requests

GitHub Actions in this repository must be pinned to a full commit SHA (`gh api repos/<owner>/<repo>/commits/<tag> --jq .sha`), or the workflow fails before it starts.

- [ ] One concern per PR, in line with the current milestone.
- [ ] The three quality gates pass and new logic has tests, including incomplete and error cases.
- [ ] CLI and TUI share one service, and local navigation never queries Snowflake.
- [ ] No secrets, customer data or real metadata; docs and examples are updated.
- [ ] The description covers the problem, design, behavior change, testing, limitations, and security and privacy impact.

## Security issues

Do not report vulnerabilities in public issues; use the repository's private security contact (for example GitHub private vulnerability reporting) and include no real customer metadata.

## License

Contributions are submitted under the project's [MIT License](LICENSE.md).
