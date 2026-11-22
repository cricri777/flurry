# Flurry

> A local-first Snowflake RBAC explorer for your terminal.

> [!WARNING]
> Work in progress and unstable: everything may change without notice.

New here? [Install](docs/getting-started.md#2-install-flurry) a release binary and follow [Getting started](docs/getting-started.md); day-to-day usage is in [Usage](docs/usage.md) and the plan is in the [Roadmap](ROADMAP.md).

Flurry syncs Snowflake users, account roles, warehouses and grants into a local SQLite snapshot, then lets you search and navigate role relationships offline. It targets platform engineers and administrators of large accounts, and always shows when data was collected, where it came from, and whether it may be incomplete or stale.

**Principle: read-only, local-first, offline-capable.** Flurry never executes Snowflake DDL or DCL.

## Why Flurry?

It answers questions that are slow to answer in Snowsight:

- Which roles does a user hold directly, and which do they inherit?
- Which roles inherit a role, and which users hold it?
- Which roles have `USAGE`, `MONITOR`, `OPERATE` or other privileges on a warehouse, and through which role path?
- What changed between two snapshots?

It does not replace Snowsight, Snowflake CLI or Snowflake's authorization model; it only explores metadata the connected role is allowed to see.

## Non-goals (v1)

- Any mutation: `GRANT`, `REVOKE`, `CREATE`, `ALTER`, `DROP`.
- Proving every session's effective permissions.
- Object-level grants (tables, schemas, databases, stages), except to model role relationships.
- Warehouse runtime, query history, cost or performance metrics.
- Telemetry or sending metadata to any hosted service.
- Database, application, instance and service roles; unsupported relationships are reported, never silently dropped.

## Interface

`flurry` opens a TUI that reads the latest local snapshot and never syncs on its own; the same application services back the CLI.

- Sections: dashboard, users, roles, warehouses, access explorer, snapshots, global search.
- Local queries are the default; only `sync` contacts Snowflake, and `--offline` forbids any connection.
- Output formats are `table`, `json` and `csv`; progress goes to stderr, results to stdout.
- Stale, partial or permission-limited results are marked in human output and in JSON.
- Commands and keys: see [Usage](docs/usage.md); TUI key rules are in [CONTRIBUTING.md](CONTRIBUTING.md#vim-friendly-tui).

## Snowflake metadata collection

Flurry mixes `SHOW` commands (current state, targeted refresh) with `SNOWFLAKE.ACCOUNT_USAGE` views (bulk grants); queries and schema are in [Snowflake metadata](docs/snowflake-metadata.md).

| Entity | Primary source |
|---|---|
| User | `SHOW USERS` |
| Account role | `SHOW ROLES` |
| Warehouse | `SHOW WAREHOUSES` |
| User-role assignment | `ACCOUNT_USAGE.GRANTS_TO_USERS`; targeted `SHOW GRANTS TO USER` |
| Role hierarchy edge | `ACCOUNT_USAGE.GRANTS_TO_ROLES`; targeted `SHOW GRANTS TO ROLE` |
| Warehouse privilege | `ACCOUNT_USAGE.GRANTS_TO_ROLES`; targeted `SHOW GRANTS ON WAREHOUSE` |

Rules:

- Use explicit column lists, never `SELECT *`, and keep only current grants (`DELETED_ON IS NULL`).
- Do not collect user email addresses; names are still sensitive and protected locally.
- Keep raw Snowflake grant direction and source fields so the normalized graph stays auditable.
- Quote identifiers with a dedicated function and paginate `SHOW USERS` / `SHOW ROLES` with `LIMIT ... FROM ...`.
- `SHOW` results only include objects the active role can see, and `SHOW USERS` may return `NULL` properties; never claim a full inventory.
- `SHOW GRANTS TO USER` omits the implicit `PUBLIC` role.
- Account Usage grants lag by up to about 120 minutes and cover a subset of objects.
- Never present a fresh `SHOW` merged with stale bulk data as one uniformly fresh snapshot.
- Record `collected_at`, source, latency and status per dataset, and errors per section; a failed refresh must not erase a good snapshot.
- Distinguish `not found`, `not visible`, `not collected`, `unsupported` and `no grants`; an empty result is not proof of no grants.
- Make the query warehouse configurable, show it before a sync, and never create, resume or resize a warehouse.

## RBAC graph semantics

This is the most important domain rule in Flurry.

- Nodes: `USER`, `ACCOUNT_ROLE`, `WAREHOUSE`.
- Edges: `USER_ASSIGNED_ROLE`, `ROLE_INHERITS_ROLE`, `ROLE_HAS_WAREHOUSE_PRIVILEGE` (with a privilege attribute) and a synthetic, explicitly implicit `IMPLICIT_PUBLIC_ROLE`.

`GRANT ROLE child TO ROLE parent` means the parent inherits the child, so the normalized edge is `PARENT --ROLE_INHERITS_ROLE--> CHILD`:

```text
USER: JANE
  └── USER_ASSIGNED_ROLE → ANALYST
        └── ROLE_INHERITS_ROLE → DATA_READ_ONLY
              └── ROLE_HAS_WAREHOUSE_PRIVILEGE → WH_ANALYTICS [USAGE]
```

Access rules:

- Keep direct and inherited roles distinguishable, and keep every intermediate edge so paths stay explainable.
- Traverse with visited sets to survive cycles and duplicate edges in malformed snapshots.
- Show `DEFAULT_ROLE` separately; it is not the role active in an arbitrary session.
- Treat `USAGE`, `MONITOR`, `OPERATE`, `MODIFY` and `OWNERSHIP` as distinct; never infer `USAGE` from the others.
- If a relevant grant category is missing or incomplete, answer `UNKNOWN / INCOMPLETE`, never `NO ACCESS`.
- Do not claim to reproduce live effective privileges: secondary roles, database roles, future grants, shares and ownership are out of scope.

The graph engine is pure: it works on typed in-memory nodes and edges, with no SQLite, Snowflake or UI dependency.

## Local storage

- SQLite holds the data, with an in-memory graph built from the selected snapshot at startup.
- Data is stored per account, keyed by a stable account fingerprint, and accounts are never mixed.
- Snapshots are historical and written transactionally; the last good snapshot stays until the new one is validated.
- Dataset provenance is stored per collection run, and unique constraints prevent duplicate edges.
- Warehouse runtime state is volatile: keep it separate and timestamped, and never imply it is current.
- The database lives in the OS application data directory with owner-only permissions where possible.
- Never store passwords, keys or tokens in SQLite or in exports; `flurry cache status` shows the database path.

## Authentication

Connection profiles hold no secrets; see [Usage](docs/usage.md#authentication) for the supported methods.

- Never save passwords to disk, log authentication payloads, or bypass an organization's authentication policy.
- Work with whatever visibility the configured role has; do not require `ACCOUNTADMIN`, and never request or apply grants.

## Security and responsible use

- Get authorization before connecting to an employer's or customer's account or saving its metadata locally.
- Use a dedicated read-only role with only the required visibility.
- Reject DDL and DCL in the application layer, not just in the UI.
- Treat the local database and exports as sensitive access-control information, and warn on export.
- Send no telemetry or metadata anywhere.
- Never put real production metadata in issues, pull requests, examples, screenshots or fixtures.

## Official references

- [SHOW USERS](https://docs.snowflake.com/en/sql-reference/sql/show-users), [SHOW ROLES](https://docs.snowflake.com/en/sql-reference/sql/show-roles), [SHOW WAREHOUSES](https://docs.snowflake.com/en/sql-reference/sql/show-warehouses), [SHOW GRANTS](https://docs.snowflake.com/en/sql-reference/sql/show-grants)
- [GRANTS_TO_USERS](https://docs.snowflake.com/en/sql-reference/account-usage/grants_to_users), [GRANTS_TO_ROLES](https://docs.snowflake.com/en/sql-reference/account-usage/grants_to_roles), [ACCOUNT_USAGE latency](https://docs.snowflake.com/en/sql-reference/account-usage)
- [Snowflake-Labs/sfgrantreport](https://github.com/Snowflake-Labs/sfgrantreport): static grant reports; Flurry differs by being an interactive, local-first terminal tool.

## License

[MIT](LICENSE.md)
