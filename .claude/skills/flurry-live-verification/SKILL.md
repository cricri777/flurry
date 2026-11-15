---
name: flurry-live-verification
description: Test flurry against the user's real Snowflake account using their own connection config. Use when asked to test connectivity, try a live sync, or set up a Snowflake connection (~/.config/snowflake/config.toml); picks the connection with the user and guides setup from the official docs when none exists.
---

# Live connectivity test

This tests flurry against a real account. Read the `flurry-verification` skill first for the build and offline rules.

## Rules

- Never read the config file directly, and never ask for, print or store a password, MFA code, token or key; use `connections.py`, which prints non-secret fields only.
- Live logins need secrets only the user has: give the user a command for the `!` prefix instead of running `sync` or `connection test` yourself.
- Remind the user that a one-time MFA code pasted into the chat is spent, and that the sync is read-only (`SHOW` and `SELECT`).

## 1. Find the connections

```bash
python3 .claude/skills/flurry-live-verification/connections.py
```

It honors `SNOWFLAKE_HOME`, `~/.snowflake` and `~/.config/snowflake`, marks the default, and warns when a file is readable by others.

## 2. Choose the connection

- One or more connections: ask which to use with AskUserQuestion, one option per connection name (the default first, marked recommended); with more than four, list the names and ask for the name as free text.
- No config file or no connections: set one up with the user (next section).
- A connection named in the user's message needs no question.

## 3. Set up a connection from the docs (only when none exists)

1. Fetch the current docs with the web-fetch agent rather than answering from memory:
   - https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-cli
   - https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-connections
2. Ask which login they use (AskUserQuestion), then give them the matching snippet to paste into `~/.config/snowflake/config.toml` (they edit the file, you do not ask for the secrets):

| Login | `authenticator` | Extra keys | Needs |
|---|---|---|---|
| Password and authenticator app | `username_password_mfa` | none; the password comes from the environment | MFA enrolled in Snowsight |
| Company SSO | `externalbrowser` | none | A SAML identity provider on the account |
| Key pair | `SNOWFLAKE_JWT` | `private_key_file = "~/rsa_key.p8"` | Public key registered with `ALTER USER ... SET RSA_PUBLIC_KEY` |

```toml
[connections.default]
account = "<ORG>-<ACCOUNT>"
user = "<login name>"
authenticator = "username_password_mfa"
role = "<role with ACCOUNT_USAGE access>"
warehouse = "<warehouse>"
```

3. Have them run `chmod 600 ~/.config/snowflake/config.toml`, then re-run `connections.py` to confirm it parses.
4. Prefer a read-only role over `ACCOUNTADMIN`; `sync` needs access to `SNOWFLAKE.ACCOUNT_USAGE` and a warehouse.

## 4. Hand over the live command

Build first, then give the command matching the connection's `authenticator` for the user to run with `!`; replace `<name>` with the chosen connection and `<NAME>` with its uppercase form.

```bash
# username_password_mfa: silent password prompt, then the 6-digit code when asked
! cd <repo> && read -rs SNOWFLAKE_CONNECTIONS_<NAME>_PASSWORD; export SNOWFLAKE_CONNECTIONS_<NAME>_PASSWORD; cargo run -p flurry -- connection test -p <name>

# externalbrowser or key pair: no secret to type
! cd <repo> && cargo run -p flurry -- connection test -p <name>
```

- `connection test` prints the account, user, role and warehouse and sends one `SELECT`; run `sync -p <name>` the same way when they want the full snapshot.
- Session tokens are cached under `~/.cache/flurry`, so a second command within about an hour needs no new code.

## 5. Read the result

After the user runs it, confirm with `flurry status` and `flurry roles list` (local, safe for you to run). Typical failures:

| Error | Meaning |
|---|---|
| `SAML Identity Provider account parameter` | The account has no SSO; use password with MFA or a key pair. |
| `Incorrect username or password` | Wrong password, or a user name that differs from the Snowflake login name. |
| `Connection '<name>' not found` | The name is not in `config.toml`; re-run `connections.py`. |
| `Missing 'password'` | The environment variable was not exported in the same command. |
| `ACCOUNT_USAGE` access error during `sync` | The active role lacks access to `SNOWFLAKE.ACCOUNT_USAGE`; choose another role. |
