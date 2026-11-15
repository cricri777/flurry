# Contributing to Flurry

Thanks for your interest in contributing to Flurry. Flurry is an early-stage, local-first Snowflake RBAC explorer built in Rust. Contributions should improve correctness, usability, security, or performance without compromising the project's read-only and privacy-first principles.

## Before you start

- Read the [README](README.md) for the product scope, architecture, Snowflake metadata semantics, and keyboard interaction requirements.
- Read the [Roadmap](ROADMAP.md) before starting a substantial feature. Milestones should be completed in order unless maintainers agree otherwise.
- For a large change, open an issue or discussion first to agree on scope and avoid duplicated work.
- Do not use real employer, customer, or production Snowflake metadata in issues, pull requests, screenshots, tests, or examples.

## Development status

Flurry is in Milestone 0. The only runnable code is the compatibility spike in `crates/flurry-spike`; the rest of the README describes the target design.

## Development setup

### 1. Install Rust

Install the toolchain with [rustup](https://rustup.rs) (Arch: `sudo pacman -S rustup && rustup default stable`), then check it:

```bash
cargo --version
cargo fmt --version
cargo clippy --version
```

If `fmt` or `clippy` are missing, run `rustup component add rustfmt clippy`.

### 2. Get the code and build

```bash
git clone <repo-url> flurry && cd flurry
cargo build --workspace
```

The first build downloads and compiles about 270 crates and takes a couple of minutes; later builds are incremental.

### 3. Run the spike

Run these from the repository root; the argument after `--` picks the spike step.

```bash
cargo run -p flurry-spike -- sqlite   # writes and reads spike.sqlite, no account needed
cargo run -p flurry-spike -- tui      # j/k move, / search, : command, Esc normal, q quit
cargo run -p flurry-spike -- snowflake
```

The `snowflake` step needs a dedicated test account and key-pair auth:

```bash
openssl genrsa 2048 | openssl pkcs8 -topk8 -inform PEM -nocrypt -out rsa_key.p8
openssl rsa -in rsa_key.p8 -pubout -out rsa_key.pub
```

In Snowflake, register the public key on your test user with `ALTER USER <you> SET RSA_PUBLIC_KEY='<key body without header/footer>';`, then export:

```bash
export FLURRY_SF_ACCOUNT=<ORG-ACCOUNT>      # uppercase account identifier
export FLURRY_SF_USER=<test_user>
export FLURRY_SF_PRIVATE_KEY=./rsa_key.p8   # never commit this file
export FLURRY_SF_ROLE=<read_only_role>      # optional
export FLURRY_SF_WAREHOUSE=<warehouse>      # optional, needed for ACCOUNT_USAGE queries
cargo run -p flurry-spike -- snowflake
```

Browser SSO is not supported by `snowflake-api`, so key-pair auth is the only mode for now.

### 4. Daily commands

```bash
cargo fmt --all                                   # format
cargo clippy --workspace --all-targets            # lint; fix all warnings
cargo test --workspace                            # run tests
cargo run -p <crate> -- <args>                    # run a binary crate
```

### Rust tips for newcomers

- Read [The Rust Book](https://doc.rust-lang.org/book/) chapters 1-10 first; ownership and borrowing (ch. 4) matter most here.
- `cargo check` is much faster than `cargo build` while iterating.
- Read compiler errors top to bottom; they usually include the fix.
- Avoid `unwrap()` outside tests; return `Result` and use `?`.
- Avoid `unsafe`; the only allowed use is the FFI boundary in `flurry-spike/src/sqlite.rs`, with each call checked.
- Add a dependency only when std can't do the job, with `cargo add <crate>` in the crate that needs it.

## Architecture boundaries

Each responsibility lives in its own crate under `crates/` (planned crates in italics are not created yet):

- `flurry-graph`: pure graph construction, traversals, path explanations, cycle handling, and deduplication; no I/O, storage, or Snowflake imports.
- *`flurry-domain`*: typed entities and RBAC semantics.
- *`flurry-snowflake`*: `snowflake-api` adapter, SQL statements, identifier quoting, result normalization.
- *`flurry-storage`*: SQLite migrations and repositories; never store credentials or tokens.
- *`flurry-sync`*: collection plans, progress, freshness/coverage metadata, atomic snapshot lifecycle.
- *`flurry-cli`* / *`flurry-tui`*: output formatting and terminal UI; they call application services and never query Snowflake directly.
- `flurry-spike`: throwaway Milestone 0 experiments; delete once M0 is closed.

The CLI and TUI must share the same services and local data model. Keep the graph engine testable with in-memory synthetic fixtures.

## Product and security invariants

Every contribution must preserve these requirements:

1. **Read-only v1:** no Snowflake `GRANT`, `REVOKE`, `CREATE`, `ALTER`, `DROP`, or other mutation operations.
2. **Explicit network access:** local queries and TUI navigation must not contact Snowflake. Only explicit sync or targeted-refresh actions may connect.
3. **No telemetry by default:** do not send account metadata, queries, usage events, or diagnostics to external services.
4. **No secrets at rest in Flurry's database:** never store passwords, private keys, OAuth tokens, or session tokens in SQLite or exported snapshots.
5. **Least privilege:** do not require `ACCOUNTADMIN`; handle restricted visibility gracefully.
6. **Honest completeness:** do not treat an empty or missing result as proof of no access if collection is incomplete or visibility is restricted.
7. **Provenance:** retain source and collection timestamps, source latency, row counts, warnings, and per-dataset status.
8. **Synthetic test data only:** no real production user names, role names, warehouse names, grants, or account identifiers in committed fixtures.
9. **Safe SQL:** parameterize values where possible and use a dedicated safe identifier-quoting function for identifiers. Never concatenate arbitrary input into SQL.
10. **Local protection:** treat the local database and exports as sensitive access-control metadata.

If a change could weaken any invariant, discuss it before implementation.

## Vim-friendly TUI requirements

Vim-style navigation is part of Flurry's product identity and must be treated as a supported interaction model.

- Vim mode is enabled by default.
- Keep conventional arrow keys, `Tab`, and `Shift+Tab` available.
- Support clear Normal, Insert, Visual/selection, and command-line modes.
- In Normal mode, `j`/`k` navigate lists; `h`/`l` move between panes or collapse/expand a tree; `gg`/`G` move to the start/end of a list; `Ctrl-d`/`Ctrl-u` page; `/` searches; `n`/`N` move between matches; `:` opens command-line mode; `?` opens help; `Esc` returns to Normal mode or dismisses the topmost transient view.
- `r` refreshes from the local cache only. `R` explicitly starts a Snowflake sync.
- `q` navigates back or quits only when no text input, modal, or child view is active. `:q` and `:quit` provide explicit quit commands.
- Do not intercept navigation keys while an input has focus. Insert-mode text editing must behave normally.
- Show the current mode and context-sensitive shortcuts in the status line/help.
- Keep all essential operations possible without a mouse.
- Add tests for mode changes, focus transitions, key dispatch, search behavior, list navigation, and command execution.

Do not add one-off key handlers directly to individual screens if the behavior belongs in a shared keybinding or navigation layer. Keep the bindings internally consistent and design the layer so configurable mappings can be added later.

## Coding guidelines

- Run `cargo fmt` and `cargo clippy` with zero warnings before every commit.
- Prefer small, cohesive modules and pure functions for normalization and graph operations.
- Use typed errors and `Result`; avoid `unwrap()`/`expect()` outside tests.
- Keep `unsafe` out of the code except the documented SQLite FFI boundary.
- Keep Snowflake-specific raw field names at the connector/normalization boundary; use consistent domain names internally.
- Do not swallow errors. Return typed errors with safe, useful context.
- Redact credentials and tokens from all logs and error messages.
- Avoid logging full raw metadata rows because user and role names are sensitive.
- Use stable, deterministic output ordering for tests and CLI reports.
- Make behavior explicit when data is stale, delayed, unsupported, or incomplete.
- Document assumptions about Snowflake semantics with links to official documentation.

## Testing expectations

Add or update tests for every behavior change; in Rust, put unit tests in a `#[cfg(test)] mod tests` next to the code and integration tests in the crate's `tests/` directory, then run `cargo test --workspace`.

### Unit tests

Prioritize tests for:

- Snowflake result normalization.
- Role grant direction and inherited privilege traversal.
- Direct versus inherited user-role assignments.
- `PUBLIC` as an implicit role.
- Multiple paths, cycles, and duplicate edges.
- Warehouse privilege distinctions (`USAGE`, `MONITOR`, `OPERATE`, `MODIFY`, `OWNERSHIP`).
- Incomplete data and `UNKNOWN / INCOMPLETE` behavior.
- Snapshot atomicity, rollback, and diffing.
- Case-insensitive search and identifier handling.
- Vim mode state, focus ownership, key dispatch, and command parsing.

### Integration tests

- Use a dedicated non-production Snowflake test account only.
- Require explicit opt-in, such as `FLURRY_INTEGRATION_TESTS=1`.
- Never run against a production account by default.
- Do not commit credentials or real metadata.
- Use a test role with intentionally limited visibility to validate partial-collection behavior.

### Synthetic fixtures

Use generated or hand-written synthetic data. Include large fixtures (for example, 10k users, 10k roles, and 100k+ edges) for performance tests. Ensure names are fictional and do not resemble customer production identifiers.

## Pull request checklist

Before opening a pull request:

- [ ] The change is scoped to one concern and follows the current roadmap milestone.
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.
- [ ] New logic has unit tests, including incomplete/error cases where relevant.
- [ ] CLI and TUI use the same application service rather than duplicating logic.
- [ ] Local navigation does not unexpectedly trigger Snowflake queries.
- [ ] Vim and arrow-key interactions remain consistent.
- [ ] No secrets, customer data, or real account metadata are included.
- [ ] Documentation and examples are updated.
- [ ] Any new Snowflake assumptions cite the official documentation.
- [ ] Security and privacy implications are described in the pull request.

Use a clear pull request description: explain the problem, the design, the behavior changed, how it was tested, and any known limitations.

## Reporting security issues

Do not report suspected vulnerabilities in public issues. Contact the repository maintainers privately using the security contact configured in the repository (for example, GitHub's private vulnerability reporting, if enabled). Do not include real customer Snowflake metadata in a report unless explicitly authorized and necessary.

## License

By contributing, you agree that your contributions are submitted under the project's [MIT License](LICENSE.md). If a future contribution agreement or policy is introduced, it must be documented clearly before it applies.
