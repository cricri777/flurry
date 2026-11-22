# Flurry Roadmap

This roadmap defines a deliberately incremental path from a technical proof of concept to a useful public release. Complete each milestone's exit criteria before moving on. The roadmap is a plan, not a promise about release dates.

**Status:** Milestone 0 is done in Rust instead of Bun; Milestone 1 is done apart from the live check of targeted refresh and a live sync with restricted `ACCOUNT_USAGE` access, which the maintainer runs; Milestone 2 is done; Milestone 3 is in progress (tabs, detail panes, search, themes, relationship navigation and per-user warehouse access and sync from the TUI are done).

## Product principles

- **Local-first:** metadata is stored and queried locally; local queries never require Snowflake connectivity.
- **Read-only in v1:** no `GRANT`, `REVOKE`, `CREATE`, `ALTER`, `DROP`, or other Snowflake mutation statements.
- **Explicit synchronization:** launching the TUI must not silently start a full sync.
- **Explainability:** show direct grants, inherited roles, privilege paths, source, freshness, and collection coverage.
- **Vim-friendly:** modal keyboard interaction is enabled by default; arrow keys remain supported.
- **Honest uncertainty:** incomplete visibility must produce `UNKNOWN / INCOMPLETE`, never a false definitive `NO ACCESS`.
- **Customer-data safety:** use synthetic fixtures; do not commit real account metadata.

## Milestone 0 — Runtime and dependency compatibility spike

**Goal:** prove the proposed runtime and native dependencies work together before building product features.

Tasks:

- Create a minimal Bun + TypeScript project.
- Render a basic OpenTUI screen and handle keyboard input.
- Prove Normal/Insert mode handling for `j`, `k`, `/`, `:`, and `Esc`.
- Open and write to a local SQLite database using `bun:sqlite`.
- Connect to a dedicated test Snowflake account using `snowflake-sdk` and the intended authentication method (browser SSO where configured).
- Execute one `SHOW ROLES` command and one `ACCOUNT_USAGE` query.
- Verify clean startup, shutdown, and error handling.
- Test packaging assumptions on the primary development platform.

**Exit criteria:** OpenTUI, Bun, `snowflake-sdk`, and SQLite work together in the chosen runtime. If they do not, document the failure and decide whether to use the supported Node runtime or isolate collection in a Node subprocess before proceeding.

## Milestone 1 — Read-only metadata collector

**Goal:** collect an authorized, accurately described Snowflake metadata snapshot.

Tasks:

- Add non-secret connection profiles.
- Add connection test and account identity/fingerprint.
- Collect visible users, account roles, and warehouses.
- Collect user-role assignments, role inheritance, and warehouse privileges.
- Support `SHOW` pagination for large accounts.
- Add bulk grant extraction from `ACCOUNT_USAGE` where available.
- Add targeted `SHOW` refresh for individual users, roles, and warehouses.
- Normalize source rows into typed domain entities and graph edges.
- Record source, timestamp, documented latency, row count, status, warnings, and errors for each dataset.
- Preserve the last successful snapshot if a later collection fails.

**Exit criteria:** a sync produces a durable local snapshot; partial visibility, delayed data, unsupported categories, and collection errors are reported clearly.

## Milestone 2 — Local storage, graph, and query engine

**Goal:** answer core questions locally with no Snowflake connection.

Tasks:

- Implement SQLite schema, migrations, repositories, and indexes.
- Implement atomic snapshot staging and commit/rollback behavior.
- Build an in-memory graph from a selected snapshot.
- Implement direct and inherited role traversal.
- Implement user-to-warehouse privilege path explanations.
- Represent `PUBLIC` as an implicit role edge.
- Guard against cycles and duplicate paths.
- Distinguish `NO ACCESS` from `UNKNOWN / INCOMPLETE`.
- Add local search for users, roles, and warehouses.
- Add table and JSON CLI output; add CSV where appropriate.
- Add snapshot listing and basic snapshot diff.

**Exit criteria:** all core entity and relationship queries work offline and are covered by unit tests using synthetic fixtures.

## Milestone 3 — OpenTUI explorer

**Goal:** make Flurry useful for daily metadata exploration from the terminal.

Tasks:

- Implement dashboard and freshness/coverage indicators.
- Implement Users, Roles, and Warehouses explorers.
- Implement global search and result-list virtualization.
- Implement detail panels and bidirectional relationship navigation.
- Implement Access Explorer and explainable user-to-warehouse paths.
- Implement snapshot browsing and sync progress.
- Sync from the TUI with `R`.
- Implement Vim-friendly Normal, Insert, Visual/selection, and command-line modes.
- Keep arrow keys, `Tab`, and `Shift+Tab` functional.
- Add context-sensitive keyboard help and visible mode/focus status.
- Ensure a small terminal remains usable.

**Exit criteria:** a platform engineer can perform normal user, role, warehouse, and access lookups without using Snowsight for every navigation step. Local navigation never triggers Snowflake queries.

## Milestone 4 — Snapshot comparison, performance, and public release

**Goal:** make the product reliable, documented, distributable, and ready for external users.

Tasks:

- Complete snapshot comparison and change summaries.
- Add snapshot cleanup: prune old snapshots (keep the latest N), never the latest one, from the CLI and the TUI.
- Complete JSON/CSV exports with sensitive-data warnings.
- Add synthetic fixture generator for large accounts (for example, 10k users, 10k roles, and 100k+ relationship/grant edges).
- Benchmark startup, graph construction, search, and relationship traversal.
- Test interrupted and partially failed syncs.
- Finalize security documentation and least-privilege guidance.
- Add installation/build instructions and release packaging.
- Add terminal screenshots or a short demo using synthetic data only.
- Add contribution guidance and publish the selected license.

**Exit criteria:** the v1 definition of done below is satisfied and a new contributor can build and test the project using the repository documentation.

## v1 definition of done

- [ ] `flurry` opens the OpenTUI application.
- [ ] A user can configure a non-secret connection profile.
- [ ] Browser SSO works against a dedicated test account where configured.
- [ ] `flurry sync` collects visible users, account roles, warehouses, direct user-role grants, role inheritance, and warehouse privileges.
- [ ] The application reports datasets that are partial, delayed, unsupported, or not visible.
- [ ] A successful sync creates a persistent SQLite snapshot.
- [ ] A failed sync does not destroy the last successful snapshot.
- [ ] The application can restart and query the snapshot offline.
- [ ] Users, roles, and warehouses can be searched locally.
- [ ] A role can be explored in both inheritance directions.
- [ ] A user-to-warehouse access explanation shows the role path and privilege type.
- [ ] `PUBLIC` is represented as an implicit role.
- [ ] CLI output supports at least table and JSON formats.
- [ ] Vim-style navigation is enabled by default, with arrow-key alternatives.
- [ ] Normal, Insert, Visual/selection, and command-line mode behavior is tested.
- [ ] No Snowflake mutation statements are executed anywhere in v1.
- [ ] No secrets or customer metadata are sent to an external service.
- [ ] Unit tests cover RBAC semantics and incomplete-data cases.

## Performance targets

These are initial targets to validate on a representative developer workstation, not guarantees:

- Display the dashboard from an existing local snapshot in under 1 second for a few thousand users/roles and tens or hundreds of thousands of grants.
- Common exact/prefix local searches complete in under 100 ms.
- Common relationship lookups complete in under 100 ms after graph loading.
- List views handle thousands of records without rendering every row at once.
- Full sync reports progress by dataset, rows collected, warnings, and failures.
- Record memory usage with realistic synthetic fixtures before optimizing.

## Testing strategy

### Unit tests

At minimum, test:

- Normalization of `SHOW` and Account Usage result rows.
- Current versus revoked grants (`DELETED_ON IS NULL`).
- User-to-role assignment direction.
- Role hierarchy direction (`child TO parent` means parent inherits child).
- Multi-level role inheritance.
- Multiple valid paths to the same warehouse privilege.
- Direct versus inherited grants.
- Implicit `PUBLIC` role behavior.
- Cycle protection and duplicate edge handling.
- Incomplete/unknown data behavior (never return a false definitive "no access").
- Identifier quoting and safe SQL construction.
- Snapshot diff behavior.
- Cache transaction rollback on partial/failed sync.
- Search normalization and case-insensitive lookup.
- Vim mode transitions, focus handling, list navigation, search, and command dispatch.

### Integration tests

- Use a dedicated non-production Snowflake test account only.
- Require explicit opt-in (for example, `FLURRY_INTEGRATION_TESTS=1`).
- Never run integration tests against a production account by default.
- Do not place real account metadata or credentials in fixtures or logs.
- Validate SSO connection and metadata extraction with a test role whose visibility is intentionally limited.
- Exercise `SHOW` pagination and Account Usage latency/coverage behavior where practical.

### Required synthetic RBAC scenarios

1. User assigned one role, which has a direct `USAGE` grant on a warehouse.
2. User assigned parent role; parent inherits a child role; child has `USAGE` on a warehouse.
3. Multiple inheritance paths lead to the same warehouse privilege; show all useful paths without duplicate nodes.
4. User has no known `USAGE` path, but the snapshot is complete for the relevant data; show no known access.
5. User has no known `USAGE` path, but grants collection failed; show unknown/incomplete, not no access.
6. A grant was revoked and has non-NULL `DELETED_ON`; it must not appear as current.
7. A user has the implicit `PUBLIC` role; show it as implicit.
8. A warehouse's state changes after collection; stale state must be visibly timestamped.

## Future ideas (not v1 commitments)

- Multi-account inventory and profile comparison.
- Advanced access reviews and privilege hygiene checks.
- Policy-based checks and organization-wide reporting.
- Team-shared snapshots or reports.
- Optional enterprise deployment or hosted coordination service, only if customer demand justifies it and sensitive metadata can remain under customer control.
- Configurable key mappings and additional interaction presets.

## Implementation-agent instructions

When implementing from this roadmap and `README.md`:

1. Start with Milestone 0; do not scaffold the entire application before validating runtime compatibility.
2. Keep v1 strictly read-only. Do not add Snowflake mutation commands.
3. Implement the domain model and RBAC graph as pure TypeScript modules with unit tests before building the full UI.
4. Treat the local snapshot as a cache with provenance, freshness, and completeness metadata.
5. An incomplete source must produce `UNKNOWN / INCOMPLETE`, not `NO ACCESS`.
6. Use synthetic fixtures only. Never use real employer/customer users, role names, warehouse names, or grants in committed tests or screenshots.
7. Use parameterized values and safe identifier quoting. Never concatenate raw CLI input into SQL identifiers.
8. Keep the CLI, TUI, graph engine, collector, and storage layers separate.
9. Implement and test keyboard mode behavior rather than adding Vim key aliases as an afterthought.
10. If Snowflake behavior is uncertain, consult the official documentation linked in `README.md` and record assumptions in `docs/snowflake-metadata.md`.
11. Deliver working code, tests, and updated docs in small milestones. Do not replace this roadmap with a generic starter template.
