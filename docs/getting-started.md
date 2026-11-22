# Getting started

The happy path: configure a default Snowflake connection, install `flurry`, sync once, then query users, roles and warehouses offline.

## 0. Prerequisites

Install Rust and clone the repository as described in [CONTRIBUTING.md](../CONTRIBUTING.md#development-setup), then open a shell in the repository root.

## 1. Configure a default connection

Create `~/.config/snowflake/config.toml` in the [Snowflake CLI format](https://docs.snowflake.com/en/developer-guide/snowflake-cli/connecting/configure-cli) (the usual location); the other file locations, profile selection rules and login methods are in [usage.md](usage.md#snowflake-configuration).

```bash
mkdir -p ~/.config/snowflake
$EDITOR ~/.config/snowflake/config.toml
```

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

From the repository root, install the binary into `~/.cargo/bin`:

```bash
cargo install --path crates/flurry
```

Cargo installs there, but that directory is often not on `PATH`. Add it for the current shell only:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

To make it permanent, add the same line to your shell startup file (for example `~/.bashrc` or `~/.zshrc`). Then check it:

```bash
flurry --help
```

Until `flurry` is on `PATH`, run it with `cargo run` as described in [CONTRIBUTING.md](../CONTRIBUTING.md#3-run-flurry).

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

You can also run `flurry` and press `R` to pick a profile and sync from the TUI (see [TUI keys](usage.md#tui-keys)).

The profile only affects commands that contact Snowflake (`connection test`, `sync`, `refresh`); `status` and the `list` commands read the latest local snapshot, whichever profile created it.

## 4. Query users, roles and warehouses

These commands read only the local snapshot, so they work offline.

```bash
flurry users list
flurry roles list --search analyst
flurry warehouses list --format json
```

See [usage.md](usage.md) for every command.
