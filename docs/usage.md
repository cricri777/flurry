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
| `flurry delete <id>... \| --all` | Deletes the given local snapshots, or all of them; fails without deleting anything if an id does not exist. |
| `flurry diff [<from> <to>] [-f table\|json\|csv]` | Lists the users, roles, warehouses and grants added or removed between two snapshots (default: the latest and the previous one of the same account and active role); warns on stderr when the two differ in account or active role, and `UNKNOWN / INCOMPLETE` for kinds whose dataset failed in either one. |
| `flurry connection test [-p <profile>]` | Prints the account fingerprint, user, role and warehouse of a login. |
| `flurry refresh user\|role\|warehouse <name> \| --all [-p <profile>]` | Re-reads one entity (or, with `--all`, every entity of the latest snapshot) and its grants with targeted `SHOW` commands into the latest snapshot. |
| `flurry access <user> <warehouse> [--privilege <privilege>] [-f table\|json\|csv]` | Explains offline which role path gives the user the privilege (default `USAGE`), or says `NO ACCESS` or `UNKNOWN / INCOMPLETE`; unquoted names are upper-cased like in Snowflake. |
| `flurry roles inherits <role> [--inverse] [-f table\|json\|csv]` | Lists, offline, the roles a role inherits directly or transitively (or, with `--inverse`, the roles that inherit it); warns `UNKNOWN / INCOMPLETE` on stderr when role inheritance was not collected. |
| `flurry users\|roles\|warehouses list [-s <text>] [-f table\|json\|csv]` | Lists entities from the latest snapshot, offline. |

- Only `sync`, `refresh`, `connection test` and `R` in the TUI contact Snowflake; everything else is local.
- `refresh --all` gives current (not up to 120 min delayed) grants for the entities already listed, but costs a few `SHOW` per entity; entities no longer visible are skipped, and `sync` is faster and also finds new ones.
- `refresh` stops if the login's account fingerprint differs from the latest snapshot's; run `sync` to start a new snapshot.
- Progress goes to stderr and results to stdout; errors exit with status 1.

## TUI keys

- `Tab`/`Shift+Tab` or `1`/`2`/`3` switch between Users, Roles and Warehouses; the right pane shows the selected object's details.
- `j`/`k` or Down/Up move the cursor, or select a detail line when the details have focus; `l`/Right/`Enter` focus the details and `h`/Left/`Esc` go back.
- `Ctrl+d`/`Ctrl+u` move half a page down/up and `Ctrl+f`/`Ctrl+b` move a full page down/up, in the list, in the details and in the profile picker; a page is the number of visible rows.
- In the details, `l`/Right/`Enter` on a related user, role or warehouse opens it, and `Ctrl+o` or `Backspace` returns to where you were.
- A role's details have a `▸ lineage` fold: the tree of every role that inherits it, directly or not. `L` opens it and `H` closes it (`Enter`/`l`/Space on the line toggle it too), and a role already listed above is marked `(see above)` instead of repeated.
- A role's details end with all its grants, folded by object type (`▸ TABLE (1180)`); `Enter`/`l`/Space unfolds or folds a type, and unfolded types stay open when you move to another role.
- `y` copies the name of the selected row, or the value of the selected detail line, to the clipboard through the terminal (OSC 52).
- `--theme terminal|latte|frappe|macchiato|mocha` (or `FLURRY_THEME`, or `theme` in the [config file](#flurry-configuration)) picks the colors; `terminal` (default) follows the terminal's palette and the others are [Catppuccin](https://catppuccin.com) flavors in truecolor.
- A user's details list `USAGE` access per warehouse (`ACCESS`, `NO ACCESS`, `UNKNOWN / INCOMPLETE`), like `flurry access`.
- `R` opens a profile picker over the connections of the [Snowflake config](#snowflake-configuration) (`j`/`k` choose, `Enter` syncs, `Esc` or `q` cancels). The sync runs in the background and a window shows a spinner, a step bar (`[████░░░░░░] 2/5 roles`) and the log; `Esc` or `q` hides it and you keep browsing while the status line shows the progress, `R` reopens it, `x` cancels, and `Enter` closes it when done. A sync finished while hidden reports in the status line; the new snapshot is then loaded, also from an empty database. The TUI cannot ask for an MFA passcode, so set `passcode` in the connection or reuse the cached login of a previous `flurry sync`.
- `/` filters the list by name (`Esc` clears it), `:q` or `q` quits.

## Sync

- A sync collects `SHOW USERS`, `SHOW ROLES` and `SHOW WAREHOUSES` (paged), then the grants from `SNOWFLAKE.ACCOUNT_USAGE`.
- The active role therefore needs access to `SNOWFLAKE.ACCOUNT_USAGE` and a warehouse for those queries.
- Grant data can be up to 120 minutes old; `status` labels it.
- The `ACCOUNT_USAGE` grant queries can take several minutes on large accounts; flurry polls until Snowflake returns the result and the TUI sync pane says so.
- A snapshot is written in one transaction, so a failed sync keeps the previous snapshot.
- If the active role is denied access to an `ACCOUNT_USAGE` grant view, the snapshot is still saved, `status` shows that dataset as `FAILED` with the error (its data is unknown, not empty) and `sync` exits with status 1; any other error aborts the sync.
- Only `SHOW` and `SELECT` statements are ever sent, and user email addresses are not collected.

## Local files

| Path | Content |
|---|---|
| `~/.local/share/flurry/flurry.db` | Snapshots (`$XDG_DATA_HOME` is honored); contains account metadata, so keep it private. |
| `~/.cache/flurry/session-*.json` | Reusable session tokens (`$XDG_CACHE_HOME` is honored). |

## Flurry configuration

Flurry's own settings live in `~/.config/flurry/config.toml` (`$XDG_CONFIG_HOME` is honored); the file is optional and command-line flags and environment variables override it.

```toml
theme = "mocha"
```

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

- Change summaries and sensitive-data warnings on exports.
- Search inside the TUI.
