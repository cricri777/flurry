# Snowflake metadata

Reference for the queries Flurry runs and the schema it stores; the rules are in the [README](../README.md#snowflake-metadata-collection).

## Entity scope

| Entity | Required attributes (minimum) | Primary source |
|---|---|---|
| User | name, created_on, default_role if visible, disabled if visible, comment if visible | `SHOW USERS` |
| Account role | name, created_on, owner/comment if visible, system/custom marker if derivable | `SHOW ROLES` |
| Virtual warehouse | name, owner, type, size, created_on, comment; runtime state is volatile | `SHOW WAREHOUSES` |
| User-role assignment | user, role, grant timestamp, grantor where available, expiry where available | `SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS`; targeted `SHOW GRANTS TO USER` |
| Role hierarchy edge | parent role, inherited child role, grant timestamp, grant option, grantor where available | `SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES`; targeted `SHOW GRANTS TO ROLE` |
| Warehouse privilege | role, warehouse, privilege, grant option, grantor and grant time where available | `SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES`; targeted `SHOW GRANTS ON WAREHOUSE` / `SHOW GRANTS TO ROLE` |

## Bulk queries

Use explicit column lists, keep only current grants (`DELETED_ON IS NULL`), and preserve `CREATED_ON`, `MODIFIED_ON`, `GRANTED_BY` and `GRANT_OPTION` when available.

```sql
SELECT CREATED_ON, ROLE, GRANTEE_NAME, GRANTED_BY
FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS
WHERE DELETED_ON IS NULL;

SELECT CREATED_ON, MODIFIED_ON, PRIVILEGE, GRANTED_ON, NAME, GRANTED_TO,
       GRANTEE_NAME, GRANT_OPTION, GRANTED_BY, GRANTED_BY_ROLE_TYPE
FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES
WHERE DELETED_ON IS NULL;
```

Keep only the v1 relationship types: account role to user, account role to account role, and warehouse privilege to account role. Normalize a child role granted to a parent role as parent → child, and keep the raw grant fields.

## SHOW commands

```sql
SHOW USERS;
SHOW ROLES;
SHOW WAREHOUSES;
SHOW GRANTS TO USER <quoted_user_identifier>;
SHOW GRANTS TO ROLE <quoted_role_identifier>;
SHOW GRANTS OF ROLE <quoted_role_identifier>;
SHOW GRANTS ON WAREHOUSE <quoted_warehouse_identifier>;
```

- `SHOW GRANTS TO ROLE` lists privileges and roles granted to the role.
- `SHOW GRANTS OF ROLE` lists the users and roles the role has been granted to.
- `SHOW GRANTS TO USER` omits the implicit `PUBLIC` role.
- `SHOW WAREHOUSES` lists only warehouses the active role can access.
- `SHOW USERS` may return `NULL` properties without the right visibility privileges.
- `SHOW USERS` and `SHOW ROLES` paginate with `LIMIT ... FROM ...`.

## Freshness

- `GRANTS_TO_USERS` and `GRANTS_TO_ROLES` lag by up to about 120 minutes.
- `GRANTS_TO_ROLES` covers only a subset of supported objects.
- `GRANTS_TO_USERS` covers account-role grants only, not privileges or non-account roles.
- `SHOW` commands need no running warehouse; Account Usage queries may.

## Suggested schema

A starting point, not a requirement to implement every column.

```text
profiles          id, name, account_identifier, organization_name, region, created_at
snapshots         id, profile_id, started_at, completed_at, status, app_version
collection_runs   id, snapshot_id, dataset, source, collected_at, status,
                  source_max_latency_minutes, row_count, warning, error_code
users             snapshot_id, name, default_role, disabled, created_on, comment, raw_attributes_json
roles             snapshot_id, name, owner, created_on, comment, role_kind, raw_attributes_json
warehouses        snapshot_id, name, owner, warehouse_type, size, created_on, comment,
                  runtime_state, runtime_state_collected_at, raw_attributes_json
role_assignments  snapshot_id, user_name, role_name, created_on, granted_by, expires_on, source, is_implicit
role_inheritance  snapshot_id, parent_role_name, child_role_name, created_on, grant_option, granted_by, source
warehouse_grants  snapshot_id, role_name, warehouse_name, privilege, grant_option, created_on, granted_by, source
raw_grants        snapshot_id, source, raw_row_json   (optional)
```

## References

- [SHOW USERS](https://docs.snowflake.com/en/sql-reference/sql/show-users), [SHOW ROLES](https://docs.snowflake.com/en/sql-reference/sql/show-roles), [SHOW WAREHOUSES](https://docs.snowflake.com/en/sql-reference/sql/show-warehouses), [SHOW GRANTS](https://docs.snowflake.com/en/sql-reference/sql/show-grants)
- [GRANTS_TO_USERS](https://docs.snowflake.com/en/sql-reference/account-usage/grants_to_users), [GRANTS_TO_ROLES](https://docs.snowflake.com/en/sql-reference/account-usage/grants_to_roles), [ACCOUNT_USAGE latency](https://docs.snowflake.com/en/sql-reference/account-usage)
