# Flurry

> A local-first Snowflake RBAC explorer for your terminal.

> [!WARNING]
> Work in progress and unstable: only a feasibility spike exists, and everything may change without notice.

Flurry is an interactive terminal application for exploring Snowflake users, account roles, virtual warehouses, grants, and role inheritance. It is designed for Snowflake platform engineers and administrators working in large accounts where the native UI makes relationship discovery and repeated lookups cumbersome.

Flurry synchronizes account metadata into a local SQLite database and builds an in-memory relationship graph. After the initial synchronization, users can search and navigate the local snapshot without issuing a Snowflake query for every interaction. The application must always show when data was collected, where it came from, and whether the collection may be incomplete or stale.

**Initial product principle: read-only, local-first, offline-capable.** Flurry must never execute Snowflake DDL or DCL statements in its initial release.

---

## Why Flurry?

In a large Snowflake account, an administrator may need to answer questions such as:

- Which account roles are assigned directly to a user?
- Which roles does a role inherit, and which roles inherit it?
- Which users are assigned a particular role?
- Which roles have `USAGE`, `MONITOR`, `OPERATE`, or other privileges on a warehouse?
- Which assigned role path gives a user access to a warehouse?
- Is a privilege granted directly or inherited through the role hierarchy?
- What changed between two metadata snapshots?

These questions are often repetitive, and the relevant metadata is relatively stable compared with query/runtime data. Flurry makes it possible to synchronize that metadata once, store it locally, and explore it quickly.

Flurry is **not** intended to replace Snowsight, Snowflake CLI, or Snowflake's authorization model. It is a fast, local explorer and diagnostic tool for metadata that the connected Snowflake role is authorized to see.

## Product goals

1. Make thousands of users, roles, warehouses, and grants searchable locally.
2. Make role relationships easy to navigate in both directions.
3. Explain the known grant path between a user, their assigned roles, inherited roles, and a warehouse privilege.
4. Clearly distinguish direct grants from inherited/effective relationships.
5. Preserve snapshots locally so the application remains useful offline.
6. Make synchronization explicit and transparent, including source freshness and collection coverage.
7. Provide both an interactive OpenTUI experience and scriptable CLI commands.
8. Keep the first release read-only and safe to use with least-privilege Snowflake credentials.

## Non-goals for the initial release

- Creating, altering, or dropping Snowflake users, roles, or warehouses.
- Executing `GRANT`, `REVOKE`, or any other DDL/DCL operation.
- Replacing Snowflake's authorization engine or proving every possible session's effective permissions.
- Collecting table-, schema-, database-, stage-, integration-, or other object-level grants in the first MVP, except where needed to model account-role relationships.
- Managing warehouse runtime, query history, cost, or performance metrics.
- Sending account metadata to a hosted service or collecting telemetry.
- Supporting every Snowflake role type in v1 (database roles, application roles, instance roles, service roles, etc.). Unsupported relationship types must be reported, not silently discarded.

---

## Core user experience

Running `flurry` opens the interactive terminal UI. The TUI reads from the local snapshot by default. It must not silently start a full metadata sync every time it launches.

### Main sections

- **Dashboard** — account/profile name, counts, last sync time, source and coverage/freshness warnings.
- **Users** — searchable users, user details, directly assigned account roles, default role, disabled state when available, and related roles.
- **Roles** — searchable account roles, direct members, child roles inherited by the selected role, parent roles that inherit it, and relevant warehouse grants.
- **Warehouses** — searchable virtual warehouses, basic properties, owner, and roles with relevant privileges.
- **Access explorer** — explain the known relationship path from a user to a warehouse privilege.
- **Snapshots** — list local snapshots and compare two snapshots.
- **Global search** — search users, roles, and warehouses from one input.

### Suggested TUI layout

A practical starting layout is a three-pane interface:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ FLURRY  profile: prod-readonly  account: ACME  snapshot: 2m ago  [STALE?]     │
├───────────────┬────────────────────────────────┬─────────────────────────────┤
│ NAVIGATION    │ RESULTS                        │ DETAILS                     │
│               │                                │                             │
│ > Dashboard   │ USER_NAME       DEFAULT ROLE    │ USER: JOHN_DOE              │
│   Users       │ JOHN_DOE        DATA_ANALYST    │ Status: Active              │
│   Roles       │ JANE_DOE        FINANCE_READ    │ Default role: DATA_ANALYST  │
│   Warehouses  │ SERVICE_ETL     ETL_OPERATOR    │                             │
│   Access      │ ...                            │ Direct roles                │
│   Snapshots   │                                │  DATA_ANALYST                │
│               │                                │  REPORTING_USER              │
│               │                                │                             │
│               │                                │ Inherited roles              │
│               │                                │  DATA_READ_ONLY              │
├───────────────┴────────────────────────────────┴─────────────────────────────┤
│ / search   Enter open   Tab change pane   r refresh   ? help   q quit        │
└──────────────────────────────────────────────────────────────────────────────┘
```

This is a conceptual layout, not a pixel-perfect requirement. Adapt it to terminal width and height. The interface must remain usable in smaller terminals.

### Keyboard interaction

Suggested defaults (document and keep them consistent):

- `/` — focus global search.
- `Tab` / `Shift+Tab` — move focus between panes.
- `Enter` — open selected entity or relationship.
- `Esc` — go back / close detail panel / clear search.
- `j` / `k` or arrow keys — move selection.
- `r` — refresh selected data from local cache; do not contact Snowflake.
- `R` — start an explicit synchronization and show progress.
- `g` — go to a section or entity by name.
- `?` — keyboard help.
- `q` — quit.

The UI should use viewport rendering or pagination. Do not attempt to render several thousand rows as individual visible terminal components at once.

---

## CLI interface

The CLI and TUI must use the same application services and local data model. TUI actions must not implement their own Snowflake queries.

Proposed command surface (names can be refined during implementation, but preserve the capabilities):

```bash
# Launch the interactive TUI (uses the latest local snapshot)
flurry

# Configure / inspect a connection profile
flurry profile list
flurry profile show <profile>
flurry connection test --profile <profile>

# Synchronize metadata explicitly
flurry sync --profile <profile>
flurry sync --profile <profile> --full
flurry sync --profile <profile> --users
flurry sync --profile <profile> --roles
flurry sync --profile <profile> --warehouses
flurry sync --profile <profile> --grants

# Inspect the local cache
flurry cache status --profile <profile>
flurry cache clear --profile <profile>

# Search and inspect entities (local by default)
flurry users list --search <text> --format table
flurry users show <user-name> --format json
flurry roles list --search <text>
flurry roles show <role-name>
flurry roles members <role-name>
flurry roles grants <role-name>
flurry warehouses list --search <text>
flurry warehouses show <warehouse-name>

# Relationship / access exploration
flurry access user <user-name>
flurry access warehouse <warehouse-name>
flurry explain user <user-name> --warehouse <warehouse-name>
flurry graph role <role-name>

# Snapshot history
flurry snapshots list
flurry snapshots diff <snapshot-a> <snapshot-b>

# Export local metadata
flurry export --format json --output ./flurry-export.json
flurry export --format csv --entity users --output ./users.csv
```

### CLI behavior requirements

- Local queries are the default. A user must explicitly run `sync` to contact Snowflake.
- Provide `--profile` and `--format table|json|csv` consistently where appropriate.
- Provide a clear non-zero exit status for errors and a predictable JSON schema for machine-readable output.
- Send progress and diagnostic messages to stderr; reserve stdout for command output when a format is requested.
- Do not print credentials, private keys, tokens, or full connection strings.
- Clearly mark stale, partial, or permission-limited results in human-readable output and include equivalent metadata in JSON.
- Add `--offline` as an explicit option for commands where the behavior could otherwise be ambiguous. Offline mode must never attempt to connect.

---

## Snowflake metadata collection

Flurry needs a hybrid metadata collection strategy. It should use `SHOW` commands for operational/current metadata and targeted refreshes, and `SNOWFLAKE.ACCOUNT_USAGE` views for set-oriented grant extraction when the connected role has access.

### Initial entity scope

| Entity | Required attributes (minimum) | Primary source |
|---|---|---|
| User | name, created_on, default_role if visible, disabled if visible, comment if visible | `SHOW USERS` |
| Account role | name, created_on, owner/comment if visible, system/custom marker if derivable | `SHOW ROLES` |
| Virtual warehouse | name, owner, type, size, created_on, comment; runtime state is volatile | `SHOW WAREHOUSES` |
| User-role assignment | user, role, grant timestamp, grantor where available, expiry where available | `SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS`; targeted `SHOW GRANTS TO USER` |
| Role hierarchy edge | parent role, inherited child role, grant timestamp, grant option, grantor where available | `SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES`; targeted `SHOW GRANTS TO ROLE` |
| Warehouse privilege | role, warehouse, privilege, grant option, grantor and grant time where available | `SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES`; targeted `SHOW GRANTS ON WAREHOUSE` / `SHOW GRANTS TO ROLE` |

Do not collect user email addresses by default. User names and role names are still sensitive account metadata and must be protected locally.

### Recommended source queries

Use explicit column lists; do not use `SELECT *` in production code. Filter historical grant rows to current grants (`DELETED_ON IS NULL`). Preserve `CREATED_ON`, `MODIFIED_ON`, `GRANTED_BY`, and `GRANT_OPTION` when available.

Example for user-role grants:

```sql
SELECT
  CREATED_ON,
  ROLE,
  GRANTEE_NAME,
  GRANTED_BY
FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS
WHERE DELETED_ON IS NULL;
```

Example for grants to roles, including role hierarchy and warehouse privileges:

```sql
SELECT
  CREATED_ON,
  MODIFIED_ON,
  PRIVILEGE,
  GRANTED_ON,
  NAME,
  GRANTED_TO,
  GRANTEE_NAME,
  GRANT_OPTION,
  GRANTED_BY,
  GRANTED_BY_ROLE_TYPE
FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES
WHERE DELETED_ON IS NULL;
```

The collector must filter and normalize the rows in application code (or add narrowly scoped predicates) to the v1 relationship types it supports:

- Account role granted to a user.
- Account role granted to another account role.
- Account role granted a privilege on a virtual warehouse.

For a row where a child account role is granted to a parent account role, normalize the effective inheritance direction as **parent role → inherited child role**. Keep the raw Snowflake grant direction and source fields as well, so the normalized graph can always be audited.

### SHOW commands and targeted refresh

Use these commands for discovery and targeted refresh:

```sql
SHOW USERS;
SHOW ROLES;
SHOW WAREHOUSES;
SHOW GRANTS TO USER <quoted_user_identifier>;
SHOW GRANTS TO ROLE <quoted_role_identifier>;
SHOW GRANTS OF ROLE <quoted_role_identifier>;
SHOW GRANTS ON WAREHOUSE <quoted_warehouse_identifier>;
```

Important behavior to account for:

- `SHOW GRANTS TO ROLE` lists privileges and roles granted to that role.
- `SHOW GRANTS OF ROLE` lists users and roles to which that role has been granted.
- `SHOW GRANTS TO USER` lists roles granted to a user; the automatically available `PUBLIC` role is not listed.
- Some `SHOW` commands only return objects the active role is allowed to see.
- `SHOW WAREHOUSES` only lists warehouses for which the active role has access privileges. It does not guarantee a full-account warehouse inventory.
- `SHOW USERS` can return user names while other user properties are `NULL` if the active role lacks the required visibility privileges.
- `SHOW USERS` and `SHOW ROLES` support `LIMIT ... FROM ...` pagination. Implement pagination correctly for large accounts; do not assume the first result page contains all entities.
- Quote identifiers safely. Never concatenate arbitrary user-supplied text into SQL identifiers without a dedicated Snowflake identifier-quoting function.

### Freshness and limitations

Flurry must expose data provenance and freshness per collection, not just one global "last sync" timestamp.

`SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS` and `GRANTS_TO_ROLES` can have up to approximately 120 minutes of latency. These are useful for bulk extraction, but they are not real-time sources. `GRANTS_TO_ROLES` documents that it represents a subset of supported objects. `GRANTS_TO_USERS` contains account-role grants to users but does not include grants of privileges and non-account roles to users.

Therefore:

1. Record `collected_at`, `source`, `source_max_latency_minutes` (when documented), and collection status for every dataset.
2. Label Account Usage grant data as potentially delayed.
3. Offer targeted `SHOW` refresh for an individual user, role, or warehouse when the user requests more current information.
4. Never silently merge a fresh `SHOW` result with stale bulk data and present the combination as one uniformly fresh snapshot.
5. Display source-specific timestamps and warnings.
6. Distinguish `not found`, `not visible`, `not collected`, `unsupported`, and `no grants` wherever possible.
7. Do not interpret an empty result as proof that an entity has no grants if collection permissions or source coverage are incomplete.
8. Record errors per collection section. A failed warehouse refresh must not erase a previously successful users/roles snapshot.

`SHOW` command results may be more current, but their visibility is based on the active role. Use the active Snowflake role as the source of truth for what Flurry can see; do not claim that it has enumerated the complete account unless the source and privileges support that claim.

### Query cost

`SHOW` commands do not require a running warehouse. Queries against `SNOWFLAKE.ACCOUNT_USAGE` are regular SQL queries and may require an active warehouse, depending on the session/account setup. Make the query warehouse configurable and show the configured warehouse before a sync. Never create, resume, resize, or otherwise modify a warehouse automatically.

---

## RBAC graph semantics

This is the most important domain rule in Flurry. Implement it explicitly and cover it with unit tests.

### Node types in v1

- `USER`
- `ACCOUNT_ROLE`
- `WAREHOUSE`

### Edge types in v1

- `USER_ASSIGNED_ROLE`: user → directly assigned account role.
- `ROLE_INHERITS_ROLE`: parent account role → child account role whose privileges it inherits.
- `ROLE_HAS_WAREHOUSE_PRIVILEGE`: account role → warehouse, with a privilege attribute such as `USAGE`, `MONITOR`, `OPERATE`, `MODIFY`, or `OWNERSHIP`.
- `IMPLICIT_PUBLIC_ROLE`: synthetic user → `PUBLIC` role edge. Mark it as implicit and do not pretend Snowflake returned it as a direct grant.

### Role hierarchy direction

Snowflake statement:

```sql
GRANT ROLE child_role TO ROLE parent_role;
```

This means the parent role inherits the privileges of the child role. The normalized traversal edge should therefore be:

```text
PARENT_ROLE --ROLE_INHERITS_ROLE--> CHILD_ROLE
```

For example:

```text
USER: JANE
  └── USER_ASSIGNED_ROLE → ANALYST
        └── ROLE_INHERITS_ROLE → DATA_READ_ONLY
              └── ROLE_HAS_WAREHOUSE_PRIVILEGE → WH_ANALYTICS [USAGE]
```

The path explains why a role assignment may provide a warehouse privilege. Preserve all intermediate edges to make this explainable.

### Access calculation rules

- Direct assignments and inherited roles must remain distinguishable.
- Traverse role inheritance from an assigned parent role toward child roles.
- Avoid cycles and duplicate traversal using visited sets. Even if the expected role graph is acyclic, the implementation must be defensive against malformed or incomplete snapshots.
- Calculate reachable roles and paths; do not collapse everything into a flat list that loses the explanation path.
- For a warehouse, list the direct grants on that warehouse and the inherited role paths that reach those grants.
- Include `PUBLIC` as an implicit role in the user access graph, clearly marked.
- Show `DEFAULT_ROLE` separately from assigned roles. A default role is not proof of the role currently active in an arbitrary Snowflake session.
- Do not claim to reproduce every live Snowflake session's effective privileges. Secondary-role settings, active primary role, database roles, ownership, global privileges, future grants, shares, and other Snowflake features can affect authorization beyond the initial model.
- If the snapshot is incomplete or missing a relevant grant category, return `UNKNOWN / INCOMPLETE` rather than `NO ACCESS`.
- For warehouse usage, distinguish `USAGE` from `MONITOR`, `OPERATE`, `MODIFY`, and `OWNERSHIP`. Do not infer `USAGE` merely because a role can monitor or operate a warehouse.

### Core graph queries

The graph engine should expose pure functions similar to:

```ts
getUserDirectRoles(userName): Role[]
getRoleInheritedRoles(roleName): Role[]
getRoleParents(roleName): Role[]
getUsersForRole(roleName): User[]
getWarehouseGrants(warehouseName): WarehouseGrant[]
getUserWarehouseAccess(userName, warehouseName): AccessExplanation
getRolePaths(userName, warehouseName): RolePath[]
```

Keep this package independent from OpenTUI, SQLite, and the Snowflake driver. It should operate on typed in-memory nodes and edges so it can be tested using fixtures.

---

## Local storage and cache

Use SQLite as the persistent local store and an in-memory graph/index for fast traversal after startup.

### Storage requirements

- Store data per Snowflake account/profile. Never mix account snapshots.
- Use a stable account identity/fingerprint (account identifier plus organization/region fields when available), not merely a display label.
- Store snapshot metadata and source metadata per dataset.
- Keep historical snapshots so users can compare changes.
- Use transactions or staging tables so a failed sync does not destroy the last known-good snapshot.
- Keep the previous successful snapshot available until the new snapshot is fully validated and committed.
- Create appropriate indexes on normalized entity names, edge endpoints, grant types, and snapshot IDs.
- Use SQLite FTS5 if available for fast global search; provide a normal indexed-search fallback if FTS5 is unavailable.
- Build the in-memory graph from the selected snapshot on application startup.
- Provide cache inspection, export, and clear commands.

### Suggested schema

This is a starting point, not a requirement to implement every column immediately.

```text
profiles
  id, name, account_identifier, organization_name, region, created_at

snapshots
  id, profile_id, started_at, completed_at, status, app_version

collection_runs
  id, snapshot_id, dataset, source, collected_at, status,
  source_max_latency_minutes, row_count, warning, error_code

users
  snapshot_id, name, default_role, disabled, created_on, comment,
  raw_attributes_json

roles
  snapshot_id, name, owner, created_on, comment, role_kind,
  raw_attributes_json

warehouses
  snapshot_id, name, owner, warehouse_type, size, created_on, comment,
  runtime_state, runtime_state_collected_at, raw_attributes_json

role_assignments
  snapshot_id, user_name, role_name, created_on, granted_by, expires_on,
  source, is_implicit

role_inheritance
  snapshot_id, parent_role_name, child_role_name, created_on,
  grant_option, granted_by, source

warehouse_grants
  snapshot_id, role_name, warehouse_name, privilege, grant_option,
  created_on, granted_by, source

raw_grants (optional but recommended)
  snapshot_id, source, raw_row_json
```

Use composite keys or unique constraints to prevent duplicate normalized edges in a snapshot. Preserve source rows where practical for traceability.

### Runtime metadata versus stable metadata

Warehouse runtime state (`STARTED`, `SUSPENDED`, `RESIZING`, etc.) can change frequently. Keep it separate from relatively stable warehouse identity and configuration fields. Show its own timestamp and never use an old cached runtime state to imply the warehouse is currently running or suspended.

### Local file security

- Store the database in the user's OS-appropriate application data directory.
- Restrict directory/file permissions where the platform allows (e.g. owner-only permissions on Unix-like systems).
- Never store Snowflake passwords, private keys, OAuth tokens, or session tokens in SQLite.
- Never store secrets in the exported metadata snapshot.
- Do not send metadata, query text, or telemetry to any external service.
- Make the local database path discoverable with `flurry cache status`.
- Add an optional `--no-persist` mode in a later release if there is demand; it is not required for the first MVP.

---

## Authentication and connection profiles

Use the official Snowflake Node.js driver (`snowflake-sdk`) behind a connector abstraction.

Initial authentication priority:

1. Browser-based SSO using `authenticator: EXTERNALBROWSER` where supported by the organization's Snowflake configuration.
2. Key-pair authentication as an optional profile mode.
3. OAuth or other authentication modes only when explicitly configured and supported.

Do not implement password prompts that save passwords to disk. Do not invent or bypass an organization's authentication policy. Use Snowflake's supported authentication configuration.

Example conceptual profile (the exact config format can be refined):

```toml
[profiles.work]
account = "<account_identifier>"
user = "<snowflake_user>"
role = "<read_only_metadata_role>"
authenticator = "EXTERNALBROWSER"
warehouse = "<optional_query_warehouse>"
```

The profile must not contain secrets. Secret material should be provided through the Snowflake driver/OS credential mechanisms or environment variables. Never log authentication payloads.

Expose a connection test that reports account identity, current user, current role, and current warehouse when available, without exposing tokens.

### Least privilege

Flurry must work with whatever visibility the configured Snowflake role has. Do not require `ACCOUNTADMIN` as a blanket requirement. If an administrator wants a fuller inventory, they must have the appropriate Snowflake privileges granted by their organization. Explain missing privileges and incomplete visibility; do not request or apply grants automatically.

---

## Technology choices

The initial implementation should use TypeScript and OpenTUI.

| Area | Proposed technology | Notes |
|---|---|---|
| Runtime/package manager | Bun | Use current OpenTUI-supported Bun version. |
| Language | TypeScript, ESM | Strict compiler settings. |
| TUI | `@opentui/react` + `@opentui/core` | React bindings for component-based screens. |
| Snowflake connector | `snowflake-sdk` | Wrap behind an interface and test runtime compatibility early. |
| Local database | `bun:sqlite` | Embedded database; no external DB server. |
| Graph engine | Plain TypeScript | Pure and independently testable. |
| CLI parsing | Commander (or another small mature parser) | Keep CLI commands thin. |
| Validation | Zod | Validate config and normalized data boundaries. |
| Logging | Structured local logs | Redact secrets and avoid storing raw sensitive query results in logs. |
| Tests | Bun test runner | Unit tests, fixtures, and opt-in integration tests. |

OpenTUI currently documents Bun 1.3.14+ as supported. Node.js is also supported with a recent Node runtime and experimental FFI, but Bun should be the default for this project. **Before building the entire application, create a small proof of compatibility between Bun, the selected OpenTUI renderer, `snowflake-sdk`, and `bun:sqlite`.** If the Snowflake driver is not reliable under Bun, decide early whether to use the documented Node.js OpenTUI path or isolate metadata collection in a small Node subprocess. Do not discover this late in the project.

Keep packages separated by responsibility:

```text
flurry/
├── src/
│   ├── cli/                 # command definitions and output formatting
│   ├── tui/                 # OpenTUI screens, components, navigation
│   ├── application/         # use cases / orchestration
│   ├── domain/              # entities, types, graph semantics
│   ├── snowflake/           # driver adapter, SQL, result normalization
│   ├── sync/                # collection plans, sync lifecycle, progress
│   ├── storage/             # SQLite repositories and migrations
│   ├── graph/               # graph construction and traversals
│   ├── search/              # local search/indexing
│   └── config/              # profiles and paths
├── tests/
│   ├── fixtures/            # synthetic metadata only
│   ├── unit/
│   └── integration/         # opt-in, requires explicit test account
├── docs/
│   ├── architecture.md
│   ├── snowflake-metadata.md
│   └── security.md
├── README.md
├── LICENSE                 # MIT
├── package.json
├── bun.lock
└── tsconfig.json
```

Avoid coupling the Snowflake driver, storage, graph engine, or CLI to OpenTUI components.

---

## Performance expectations

The product exists to make local exploration fast. Treat these as initial targets to validate, not guarantees:

- Open an existing local snapshot and display the dashboard in under 1 second on a typical developer workstation, for a few thousand users/roles and tens or hundreds of thousands of grants.
- Local exact/prefix search should feel instantaneous (target under 100 ms for common searches).
- Common role/user/warehouse relationship lookups should normally complete under 100 ms after the graph is loaded.
- TUI list views must support thousands of records without freezing or rendering every row at once.
- A full sync must show progress by dataset, rows collected, warnings, and failures.
- Memory use should be measured with realistic synthetic fixtures; do not optimize prematurely.

Add a synthetic fixture generator that can create large accounts (e.g. 10k users, 10k roles, 100k+ relationship/grant edges) without any real customer metadata.

---

## Testing strategy

### Unit tests

Test at minimum:

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

### Integration tests

- Use a dedicated non-production Snowflake test account only.
- Require explicit opt-in (for example `FLURRY_INTEGRATION_TESTS=1`).
- Never run integration tests against a production account by default.
- Do not place real account metadata or credentials in fixtures or logs.
- Validate SSO connection and metadata extraction with a test role whose visibility is intentionally limited.

### Required synthetic scenarios

1. User assigned one role, which has a direct `USAGE` grant on a warehouse.
2. User assigned parent role; parent inherits a child role; child has `USAGE` on a warehouse.
3. Multiple inheritance paths lead to the same warehouse privilege; show all useful paths without duplicate nodes.
4. User has no known `USAGE` path, but the snapshot is complete for the relevant data; show no known access.
5. User has no known `USAGE` path, but grants collection failed; show unknown/incomplete, not no access.
6. A grant was revoked and has non-NULL `DELETED_ON`; it must not appear as current.
7. A user has the implicit `PUBLIC` role; show it as implicit.
8. A warehouse's state changes after collection; stale state must be visibly timestamped.

---

## Milestones

### Milestone 0 — Compatibility spike

- Create a minimal Bun + TypeScript + OpenTUI application.
- Connect to a test Snowflake account using `snowflake-sdk` and browser SSO.
- Execute one `SHOW ROLES` command and one Account Usage query.
- Open and write to a local SQLite database.
- Confirm packaging and shutdown behavior.

**Exit criteria:** all four dependencies work together in the intended runtime.

### Milestone 1 — Read-only metadata collector

- Profiles and connection test.
- Collect users, account roles, warehouses, user-role assignments, role hierarchy, warehouse privileges.
- Normalize source rows into typed domain entities.
- Store source timestamps, row counts, coverage, and warnings.

**Exit criteria:** a sync produces a durable local snapshot and reports its limitations honestly.

### Milestone 2 — Local query engine

- SQLite migrations and repositories.
- In-memory relationship graph.
- User/role/warehouse search and detail queries.
- Explain user-to-warehouse privilege paths.
- CLI table/JSON/CSV outputs.

**Exit criteria:** core queries work without a Snowflake connection.

### Milestone 3 — OpenTUI explorer

- Dashboard.
- Users, roles, and warehouses explorers.
- Global search and keyboard navigation.
- Detail panels and relationship traversal.
- Sync progress and warnings.

**Exit criteria:** a developer can use Flurry for normal metadata lookups without Snowsight.

### Milestone 4 — Snapshot diff and polish

- Snapshot listing and comparison.
- Export.
- Synthetic large-account benchmark.
- Documentation, screenshots/GIF, release packaging.

**Exit criteria:** first usable public open-source release.

---

## Definition of done for v1

Flurry v1 is ready when all of the following are true:

- [ ] `flurry` opens the OpenTUI application.
- [ ] A user can configure a non-secret connection profile.
- [ ] Browser SSO works against a dedicated test account (where configured).
- [ ] `flurry sync` collects users, account roles, visible warehouses, direct user-role grants, role inheritance, and warehouse privileges.
- [ ] The application clearly reports datasets that are partial, delayed, unsupported, or not visible.
- [ ] A successful sync creates a persistent SQLite snapshot.
- [ ] A failed sync does not destroy the last successful snapshot.
- [ ] The application can restart and query the snapshot offline.
- [ ] Users, roles, and warehouses can be searched locally.
- [ ] A role can be explored in both inheritance directions.
- [ ] A user-to-warehouse access explanation shows the role path and privilege type.
- [ ] `PUBLIC` is represented as an implicit role.
- [ ] CLI output supports at least table and JSON formats.
- [ ] No Snowflake mutation statements are executed anywhere in v1.
- [ ] No secrets or customer metadata are sent to an external service.
- [ ] Unit tests cover the RBAC semantics and incomplete-data cases.

---

## Security and responsible use

Flurry is intended for administrators, but being able to query metadata does not mean that every user or workstation is authorized to store or export it.

- Obtain explicit authorization before connecting Flurry to an employer's or customer's Snowflake account or saving account metadata locally.
- Use a dedicated read-only role with only the required visibility and query privileges.
- Do not require or recommend `ACCOUNTADMIN` as the default connection role.
- Keep the first release strictly read-only. Reject DDL/DCL commands in the application layer; do not merely hide write commands from the UI.
- Do not store authentication secrets in the local metadata database.
- Protect local cache files and exports as sensitive access-control information.
- Do not send telemetry or metadata anywhere by default.
- Make export an explicit user action and warn that exported files may reveal account structure and access relationships.
- Never include real production metadata in GitHub issues, pull requests, examples, screenshots, or test fixtures.

---

## Open-source and future commercial direction

The initial goal is to build a genuinely useful open-source developer/admin tool, not to monetize prematurely.

Possible future directions after the local-first MVP proves useful:

- Multi-account profile comparison.
- Snapshot diff and grant change history.
- Access review reports.
- Explainable access analysis and privilege hygiene checks.
- Exportable audit evidence.
- Team-shared policies and reports.
- Centralized inventory for organizations with many Snowflake accounts.
- Optional hosted or enterprise deployment, only if customers need it.

The local core should remain useful without a Flurry account, cloud service, or subscription. Any commercial features must preserve the option to keep sensitive metadata inside the customer's environment.

### Potential positioning

**Flurry — Explore Snowflake access relationships at terminal speed.**

A local-first Snowflake metadata and RBAC explorer for platform engineers, data engineers, and account administrators.

---

## Development instructions for implementation agents

When implementing from this README:

1. Start with **Milestone 0**. Do not scaffold the entire application before verifying runtime compatibility between Bun, OpenTUI, the Snowflake Node.js driver, and SQLite.
2. Keep the first release read-only. Do not add `GRANT`, `REVOKE`, `CREATE`, `ALTER`, or `DROP` functionality.
3. Implement the domain model and RBAC graph as pure TypeScript modules with unit tests before building the full UI.
4. Treat the local snapshot as a cache with provenance, not as unquestionable truth. Preserve per-dataset freshness and completeness.
5. Do not silently infer missing permissions. An incomplete source must produce an `UNKNOWN / INCOMPLETE` result rather than `NO ACCESS`.
6. Use synthetic fixtures only. Never use real employer/customer users, role names, warehouse names, or grants in committed tests or screenshots.
7. Use parameterized values and a safe identifier-quoting function for Snowflake SQL. Never concatenate raw CLI input into SQL.
8. Keep the CLI, TUI, graph engine, collector, and storage layers separate.
9. Add tests for each behavior before considering that milestone complete.
10. If a Snowflake metadata behavior is uncertain, consult the official documentation linked below and record the assumption in `docs/snowflake-metadata.md` rather than guessing.

The implementation agent should produce working code, tests, and updated documentation in small milestones. Do not replace this specification with a generic starter template.

---

## Official references

Snowflake documentation:

- [SHOW USERS](https://docs.snowflake.com/en/sql-reference/sql/show-users)
- [SHOW ROLES](https://docs.snowflake.com/en/sql-reference/sql/show-roles)
- [SHOW WAREHOUSES](https://docs.snowflake.com/en/sql-reference/sql/show-warehouses)
- [SHOW GRANTS](https://docs.snowflake.com/en/sql-reference/sql/show-grants)
- [ACCOUNT_USAGE — GRANTS_TO_USERS](https://docs.snowflake.com/en/sql-reference/account-usage/grants_to_users)
- [ACCOUNT_USAGE — GRANTS_TO_ROLES](https://docs.snowflake.com/en/sql-reference/account-usage/grants_to_roles)
- [ACCOUNT_USAGE overview and latency](https://docs.snowflake.com/en/sql-reference/account-usage)
- [Snowflake Node.js driver authentication](https://docs.snowflake.com/en/developer-guide/node-js/nodejs-driver-authenticate)

OpenTUI documentation:

- [OpenTUI documentation](https://opentui.com/docs/)
- [Runtime and platform support](https://opentui.com/docs/getting-started/runtime-support/)
- [OpenTUI React bindings](https://opentui.com/docs/bindings/react/)
- [Keyboard input](https://opentui.com/docs/core-concepts/keyboard/)

Related project to study:

- [Snowflake-Labs/sfgrantreport](https://github.com/Snowflake-Labs/sfgrantreport) — existing Snowflake grants reporting and visualization tool. Flurry should differentiate through its local-first interactive terminal workflow, not duplicate static report generation.

---

## License

MIT
