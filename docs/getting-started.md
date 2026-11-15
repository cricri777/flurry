# Getting started

The happy path: configure a default Snowflake connection, install `flurry`, sync once, then query users, roles and warehouses offline.

## 1. Configure a default connection

Create a `config.toml` in the [Snowflake CLI format](https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-cli); the file locations, profile selection rules and login methods are in [usage.md](usage.md#snowflake-configuration).

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

Find your account identifier in [Account identifiers](https://docs.snowflake.com/en/user-guide/admin-account-identifier).

## 2. Install flurry

With [Rust](https://rustup.rs) installed, run this from the repository root:

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

The profile only affects commands that contact Snowflake (`connection test`, `sync`, `refresh`); `status` and the `list` commands read the latest local snapshot, whichever profile created it.

## 4. Query users, roles and warehouses

These commands read only the local snapshot, so they work offline.

```bash
flurry users list
flurry roles list --search analyst
flurry warehouses list --format json
```

See [usage.md](usage.md) for every command.
