# Getting started

The happy path: configure a default Snowflake connection, install `flurry`, sync once, then query users, roles and warehouses offline.

## 1. Configure a default connection

Create `~/.snowflake/config.toml` (or `~/.config/snowflake/config.toml`) in the [Snowflake CLI format](https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-cli); flurry uses the connection named `default`, or the one set by `default_connection_name`.

```toml
default_connection_name = "default"

[connections.default]
account = "myorg-myaccount"
user = "MY_USER"
authenticator = "externalbrowser"
role = "MY_ROLE"
warehouse = "MY_WAREHOUSE"

[connections.test]
account = "myorg-testaccount"
user = "MY_USER"
authenticator = "externalbrowser"
```

- Find your account identifier in [Account identifiers](https://docs.snowflake.com/en/user-guide/admin-account-identifier).
- Other login methods (MFA password, key pair) are listed in [usage.md](usage.md#authentication).
- Keep the file private (`chmod 600`) and never commit it.
- The role needs access to `SNOWFLAKE.ACCOUNT_USAGE` and a warehouse, see [Sync](usage.md#sync).

## 2. Install flurry

Install [Rust](https://rustup.rs), then from the repository root:

```bash
cargo install --path crates/flurry
```

Make sure `~/.cargo/bin` is on your `PATH`, then check it:

```bash
flurry --help
```

## 3. Test the connection and sync

```bash
flurry connection test   # prints the account, user, role and warehouse
flurry sync              # reads from Snowflake into a local snapshot
flurry status            # shows what was collected and how fresh it is
```

Without `--profile`, these use the default connection. To use another section of `config.toml`, pass its name:

```bash
flurry connection test -p test
flurry sync --profile test
```

`SNOWFLAKE_DEFAULT_CONNECTION_NAME` also changes the default for the shell session. The profile only affects commands that contact Snowflake (`connection test`, `sync`, `refresh`); `status` and the `list` commands read the latest local snapshot, whichever profile created it.

## 4. Query users, roles and warehouses

These commands read only the local snapshot, so they work offline.

```bash
flurry users list
flurry roles list --search analyst
flurry warehouses list --format json
```

Run `flurry sync` again whenever you want fresher data; see [usage.md](usage.md) for every command.
