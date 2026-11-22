# Getting started

The happy path: install `flurry`, configure a default Snowflake connection, sync once, then query users, roles and warehouses offline.

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

Flurry supports Linux only (x86_64, including WSL Ubuntu 22.04 or newer). Download `flurry-<version>-x86_64-unknown-linux-gnu.tar.gz` and its `.sha256` from the [latest release](https://github.com/cricri777/flurry/releases/latest), then verify the checksum, extract the binary and put it on `PATH`:

```bash
sha256sum -c flurry-*.sha256
tar xzf flurry-*.tar.gz
mkdir -p ~/.local/bin && mv flurry ~/.local/bin/
export PATH="$HOME/.local/bin:$PATH"   # add this line to ~/.bashrc or ~/.zshrc to keep it
flurry --help
```

With Rust installed, you can build from source instead:

```bash
cargo install --git https://github.com/cricri777/flurry flurry
```

Cargo installs into `~/.cargo/bin`, which you may need to add to `PATH`. To run from a clone without installing, see [CONTRIBUTING.md](../CONTRIBUTING.md#development-setup).

### WSL

The `externalbrowser` login calls `xdg-open`, which WSL may not have. Flurry prints the SSO URL, so open it in your Windows browser, or install `wslu` and set `BROWSER=wslview`.

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
