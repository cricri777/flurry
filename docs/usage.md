# Using flurry

Flurry is a work in progress; it only reads from Snowflake and keeps everything in a local SQLite database.

## Commands

```
flurry [command]
```

| Command | What it does |
|---|---|
| `flurry` | Opens the TUI over the roles of the latest local snapshot. |
| `flurry sync [-p <profile>]` | Collects users, roles, warehouses and grants from Snowflake into a new snapshot. |
| `flurry status` | Shows the latest snapshot and the source, time and freshness of each dataset. |
| `flurry snapshots [-f table\|json\|csv]` | Lists the local snapshots, newest first, with the number of datasets that failed to collect or refresh. |
| `flurry connection test [-p <profile>]` | Prints the account fingerprint, user, role and warehouse of a login. |
| `flurry refresh user\|role\|warehouse <name> \| --all [-p <profile>]` | Re-reads one entity (or, with `--all`, every entity of the latest snapshot) and its grants with targeted `SHOW` commands into the latest snapshot. |
| `flurry access <user> <warehouse> [--privilege <privilege>] [-f table\|json\|csv]` | Explains offline which role path gives the user the privilege (default `USAGE`), or says `NO ACCESS` or `UNKNOWN / INCOMPLETE`; unquoted names are upper-cased like in Snowflake. |
| `flurry roles inherits <role> [--inverse] [-f table\|json\|csv]` | Lists, offline, the roles a role inherits directly or transitively (or, with `--inverse`, the roles that inherit it); warns `UNKNOWN / INCOMPLETE` on stderr when role inheritance was not collected. |
| `flurry users\|roles\|warehouses list [-s <text>] [-f table\|json\|csv]` | Lists entities from the latest snapshot, offline. |

- Only `sync`, `refresh` and `connection test` contact Snowflake; everything else is local.
- `refresh --all` gives current (not up to 120 min delayed) grants for the entities already listed, but costs a few `SHOW` per entity; entities no longer visible are skipped, and `sync` is faster and also finds new ones.
- Progress goes to stderr and results to stdout; errors exit with status 1.

## TUI keys

- `j`/`k` or Down/Up move the cursor; `q` quits.
- `/` and `:` collect text only for now; `Esc` returns to normal mode.

## Sync

- A sync collects `SHOW USERS`, `SHOW ROLES` and `SHOW WAREHOUSES` (paged), then the grants from `SNOWFLAKE.ACCOUNT_USAGE`.
- The active role therefore needs access to `SNOWFLAKE.ACCOUNT_USAGE` and a warehouse for those queries.
- Grant data can be up to 120 minutes old; `status` labels it.
- A snapshot is written in one transaction, so a failed sync keeps the previous snapshot.
- If the active role is denied access to an `ACCOUNT_USAGE` grant view, the snapshot is still saved, `status` shows that dataset as `FAILED` with the error (its data is unknown, not empty) and `sync` exits with status 1; any other error aborts the sync.
- Only `SHOW` and `SELECT` statements are ever sent, and user email addresses are not collected.

## Local files

| Path | Content |
|---|---|
| `~/.local/share/flurry/flurry.db` | Snapshots (`$XDG_DATA_HOME` is honored); contains account metadata, so keep it private. |
| `~/.cache/flurry/session-*.json` | Reusable session tokens (`$XDG_CACHE_HOME` is honored). |

## Snowflake configuration

Connections follow the [Snowflake CLI format](https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-cli).

- The directory is `$SNOWFLAKE_HOME`, else `~/.snowflake` if it exists, else `~/.config/snowflake`.
- `config.toml` holds `[connections.<name>]` sections and an optional `default_connection_name`; `connections.toml` replaces them when present.
- The connection is chosen by `--profile`, then `SNOWFLAKE_DEFAULT_CONNECTION_NAME`, then `default_connection_name`, then a connection named `default`.
- `SNOWFLAKE_CONNECTIONS_<NAME>_<KEY>` overrides any key; `SNOWFLAKE_ACCOUNT`, `_USER`, `_ROLE`, `_WAREHOUSE` and `_PRIVATE_KEY_FILE` fill missing ones.
- Keys read: `account`, `user`, `authenticator`, `role`, `warehouse`, `database`, `schema`, `password`, `passcode`, `private_key_file` (or `private_key_path`).

## Authentication

| `authenticator` | Needs | Notes |
|---|---|---|
| `username_password_mfa` | `password`, a TOTP code | Prompts for the code unless `passcode` is set; no Duo push or passkeys. |
| `externalbrowser` | An account with SAML SSO | Opens the browser and catches the redirect on localhost. |
| anything else or unset | `private_key_file` | Key-pair login; not yet tested against a real account. |

- Password and browser logins cache the session token until Snowflake expires it (about an hour); delete `~/.cache/flurry` to force a new login.
- Plain password login without MFA is not supported.

## Not supported yet

- Snapshot comparison and exports.
- Search inside the TUI.
