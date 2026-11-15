# flurry-spike

Feasibility spike for flurry: it checks that SQLite, Snowflake and a terminal UI work together.

## Run

```
cargo run -p flurry-spike -- <mode> [connection]
```

| Mode | What it does |
|---|---|
| `sqlite` | Inserts a row into `spike.sqlite` in the current directory and prints the row count. |
| `snowflake [connection]` | Connects and runs `SHOW ROLES` and a grants count, printing row counts only. |
| `tui` | Demo interface over four fake roles; needs a real terminal and ignores Snowflake. |

## TUI keys

- `j`/`k` or Down/Up move the cursor; `q` quits.
- `/` enters search mode and `:` enters command mode; both only collect text, so typing does nothing yet.
- `Esc` returns to normal mode and clears the text; `Enter` returns to normal mode and keeps it.

## Snowflake configuration

Connections follow the [Snowflake CLI format](https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-cli).

### Config files

- The directory is `$SNOWFLAKE_HOME`, else `~/.snowflake` if it exists, else `~/.config/snowflake`.
- `config.toml` holds `[connections.<name>]` sections and an optional top-level `default_connection_name`.
- If `connections.toml` exists it replaces the connections in `config.toml`; its sections are `[<name>]`.
- File permissions are not checked, but keep the files at `0600`.

### Choosing a connection

In priority order: the CLI argument, `SNOWFLAKE_DEFAULT_CONNECTION_NAME`, `default_connection_name`, then a connection named `default`.

### Keys

| Key | Used for |
|---|---|
| `account`, `user` | Required for every authenticator. |
| `authenticator` | Selects the login method (see below). |
| `role`, `warehouse`, `database`, `schema` | Session context; all optional. |
| `password` | Password for `username_password_mfa`. |
| `passcode` | MFA code; if unset, flurry prompts for it. |
| `private_key_file` or `private_key_path` | Private key for key-pair login; `~/` is expanded. |

Only string values are read; other keys are ignored.

### Environment overrides

- `SNOWFLAKE_CONNECTIONS_<NAME>_<KEY>` overrides any key of that connection, for example `SNOWFLAKE_CONNECTIONS_DEFAULT_PASSWORD`.
- `SNOWFLAKE_ACCOUNT`, `_USER`, `_ROLE`, `_WAREHOUSE` and `_PRIVATE_KEY_FILE` fill keys the connection does not set.

## Authentication methods

| `authenticator` | Needs | Behavior |
|---|---|---|
| `username_password_mfa` | `password`, a TOTP code | Prompts for the code (or reads `passcode`); Duo push and passkeys are not supported. |
| `externalbrowser` | An account with a SAML identity provider | Opens the browser and catches the redirect on localhost; fails on accounts without SSO. |
| anything else or unset | `private_key_file` | Key-pair (JWT) login. |

Plain password login without MFA is not supported.

## Session cache

- After a password-MFA or browser login, the session token is saved under `~/.cache/flurry/` (or `$XDG_CACHE_HOME/flurry/`) with `0600` permissions.
- Later runs reuse it, with no prompts, until Snowflake expires it (about an hour), then log in again.
- There is one cache file per account, user, role, warehouse, database and schema; delete the directory to force a fresh login.
- Key-pair login does not use the cache, since it needs no prompt.

## Safety

- Only statements starting with `SHOW` or `SELECT` are ever sent to Snowflake.
- Output is limited to row counts.

## Not supported yet

- Syncing Snowflake data into SQLite, and showing real data in the TUI.
- Password login without MFA, Duo push, passkeys, OAuth, and Snowflake token renewal.
- Chunked results beyond the first row count, and Windows or macOS config paths.
