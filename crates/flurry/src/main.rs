mod collect;
mod config;
mod detail;
mod modes;
mod session;
mod store;
mod ui;

use clap::{Args, Parser, Subcommand, ValueEnum};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use flurry_graph::Access;
use modes::{Focus, Key, State, Tab, handle_key};
use serde_json::{Value, json};
use session::Session;
use std::io::{Write, stdout};
use store::{Entity, Store, Table};

#[derive(Parser)]
#[command(
    version,
    about = "Local-first Snowflake RBAC explorer (work in progress)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Collect a metadata snapshot from Snowflake into the local database.
    Sync(Profile),
    /// Re-read one user, role or warehouse (or all of them with --all) into the latest snapshot.
    Refresh {
        #[arg(value_enum, requires = "name")]
        kind: Option<Kind>,
        /// Exact name of the entity.
        #[arg(requires = "kind")]
        name: Option<String>,
        /// Refresh every user, role and warehouse of the latest snapshot.
        #[arg(long, conflicts_with = "kind", required_unless_present = "kind")]
        all: bool,
        #[command(flatten)]
        profile: Profile,
    },
    /// Show the latest snapshot and the freshness of each dataset.
    Status,
    /// List the local snapshots, newest first.
    Snapshots {
        #[arg(short, long, value_enum, default_value = "table")]
        format: Format,
    },
    /// Compare two local snapshots; defaults to the latest and the previous one of the same account and role.
    Diff {
        /// Older snapshot id (see `flurry snapshots`).
        #[arg(requires = "to")]
        from: Option<i64>,
        /// Newer snapshot id.
        to: Option<i64>,
        #[arg(short, long, value_enum, default_value = "table")]
        format: Format,
    },
    /// Check the Snowflake connection.
    Connection {
        #[command(subcommand)]
        action: ConnectionCmd,
    },
    /// Explain, offline, how a user reaches a privilege on a warehouse.
    Access {
        user: String,
        warehouse: String,
        #[arg(short, long, default_value = "USAGE")]
        privilege: String,
        #[arg(short, long, value_enum, default_value = "table")]
        format: Format,
    },
    /// Local users from the latest snapshot.
    Users {
        #[command(subcommand)]
        action: ListCmd,
    },
    /// Local roles from the latest snapshot.
    Roles {
        #[command(subcommand)]
        action: RolesCmd,
    },
    /// Local warehouses from the latest snapshot.
    Warehouses {
        #[command(subcommand)]
        action: ListCmd,
    },
}

#[derive(Subcommand)]
enum ConnectionCmd {
    Test(Profile),
}

#[derive(Subcommand)]
enum ListCmd {
    /// List entities from the latest local snapshot.
    List(ListArgs),
}

#[derive(Subcommand)]
enum RolesCmd {
    /// List roles from the latest local snapshot.
    List(ListArgs),
    /// List the roles a role inherits, directly or transitively.
    Inherits {
        /// Exact role name; unquoted names are upper-cased like in Snowflake.
        role: String,
        /// List the roles that inherit it instead.
        #[arg(short, long)]
        inverse: bool,
        #[arg(short, long, value_enum, default_value = "table")]
        format: Format,
    },
}

#[derive(Args)]
struct Profile {
    /// Connection name from the Snowflake config.
    #[arg(short, long)]
    profile: Option<String>,
}

#[derive(Args)]
struct ListArgs {
    /// Case-insensitive name filter.
    #[arg(short, long, default_value = "")]
    search: String,
    #[arg(short, long, value_enum, default_value = "table")]
    format: Format,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    User,
    Role,
    Warehouse,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Table,
    Json,
    Csv,
}

/// Joins cells into one RFC 4180 line, quoting cells that contain a comma, quote or newline.
fn csv_line(cells: &[String]) -> String {
    let cells: Vec<String> = cells
        .iter()
        .map(|c| {
            if c.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", c.replace('"', "\"\""))
            } else {
                c.clone()
            }
        })
        .collect();
    cells.join(",")
}

fn print_table(t: &Table, format: Format) {
    match format {
        Format::Json => {
            let rows: Vec<Value> = t
                .rows
                .iter()
                .map(|r| {
                    Value::Object(
                        t.headers
                            .iter()
                            .cloned()
                            .zip(
                                r.iter()
                                    .map(|c| if c.is_empty() { Value::Null } else { json!(c) }),
                            )
                            .collect(),
                    )
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&rows).unwrap_or_default()
            );
        }
        Format::Csv => {
            println!("{}", csv_line(&t.headers));
            for r in &t.rows {
                println!("{}", csv_line(r));
            }
        }
        Format::Table => {
            let widths: Vec<usize> = (0..t.headers.len())
                .map(|i| {
                    t.rows
                        .iter()
                        .map(|r| r[i].chars().count())
                        .chain([t.headers[i].len()])
                        .max()
                        .unwrap_or(0)
                })
                .collect();
            let line = |cells: &[String]| {
                let parts: Vec<String> = cells
                    .iter()
                    .zip(&widths)
                    .map(|(c, w)| format!("{c:<w$}"))
                    .collect();
                println!("{}", parts.join("  ").trim_end());
            };
            line(&t.headers);
            for r in &t.rows {
                line(r);
            }
        }
    }
}

async fn connect(p: &Profile) -> Result<Session, String> {
    Session::connect(&config::load(p.profile.as_deref())?).await
}

async fn sync(p: &Profile) -> Result<(), String> {
    let session = connect(p).await?;
    let snap = collect::collect(&session, |what| eprintln!("Collecting {what}...")).await?;
    let id = Store::open()?.save(p.profile.as_deref().unwrap_or("default"), &snap)?;
    println!(
        "Snapshot {id} saved (account {}, role {})",
        snap.fingerprint, snap.active_role
    );
    for d in &snap.datasets {
        match &d.error {
            None => println!("  {:<22} {}", d.name, d.rows),
            Some(err) => println!("  {:<22} FAILED: {err}", d.name),
        }
    }
    if snap.datasets.iter().any(|d| d.error.is_some()) {
        return Err(
            "snapshot saved, but some datasets failed; their data is unknown, not empty.".into(),
        );
    }
    Ok(())
}

async fn refresh(kind: Option<Kind>, name: Option<String>, p: &Profile) -> Result<(), String> {
    let mut store = Store::open()?;
    if store.latest()?.is_none() {
        return Err("No snapshot yet. Run `flurry sync` first.".into());
    }
    let all = kind.is_none();
    let targets: Vec<(Entity, String)> = match (kind, name) {
        (Some(k), Some(n)) => vec![(entity_of(k), n)],
        _ => {
            let mut all = Vec::new();
            for entity in [Entity::Users, Entity::Roles, Entity::Warehouses] {
                for row in store.list(entity, "")?.rows {
                    all.push((entity, row.into_iter().next().unwrap_or_default()));
                }
            }
            all
        }
    };
    let session = connect(p).await?;
    let (mut totals, mut skipped): (Vec<collect::Dataset>, usize) = (Vec::new(), 0);
    for (entity, name) in &targets {
        let mut snap = match collect::refresh(&session, *entity, name).await {
            Ok(s) => s,
            Err(e) if all && e.ends_with(collect::NOT_FOUND) => {
                eprintln!("Skipped {e}");
                skipped += 1;
                continue;
            }
            Err(e) => return Err(e),
        };
        if all {
            // One dataset row per kind instead of one per entity, so `status` stays short.
            let total = format!("refresh all {}", label_of(*entity));
            for d in snap.datasets.drain(..) {
                match totals.iter_mut().find(|t| t.name == total) {
                    Some(t) => {
                        t.rows += d.rows;
                        t.collected_at = d.collected_at;
                    }
                    None => totals.push(collect::Dataset {
                        name: total.clone(),
                        ..d
                    }),
                }
            }
        }
        let id = store.refresh(*entity, name, &snap)?;
        println!("Refreshed {name} in snapshot {id}");
        for d in &snap.datasets {
            println!("  {:<30} {}", d.name, d.rows);
        }
    }
    for t in totals {
        println!("  {:<30} {}", t.name, t.rows);
        store.record(t)?;
    }
    if skipped > 0 {
        return Err(format!(
            "{skipped} of {} entities were skipped; run `flurry sync` to drop them",
            targets.len()
        ));
    }
    Ok(())
}

fn label_of(entity: Entity) -> &'static str {
    match entity {
        Entity::Users => "users",
        Entity::Roles => "roles",
        Entity::Warehouses => "warehouses",
    }
}

fn entity_of(kind: Kind) -> Entity {
    match kind {
        Kind::User => Entity::Users,
        Kind::Role => Entity::Roles,
        Kind::Warehouse => Entity::Warehouses,
    }
}

fn status() -> Result<(), String> {
    let store = Store::open()?;
    let Some(latest) = store.latest()? else {
        println!("No snapshot yet. Run `flurry sync`.");
        return Ok(());
    };
    let r = &latest.rows[0];
    println!(
        "Snapshot {} of profile '{}' (account {}, role {}) taken {} UTC",
        r[0], r[1], r[2], r[3], r[4]
    );
    if !r[5].is_empty() {
        println!("Account {}", r[5]);
    }
    println!();
    print_table(&store.datasets()?, Format::Table);
    Ok(())
}

fn snapshots(format: Format) -> Result<(), String> {
    print_table(&Store::open()?.snapshots()?, format);
    Ok(())
}

fn diff(ids: Option<(i64, i64)>, format: Format) -> Result<(), String> {
    let store = Store::open()?;
    let (from, to) = match ids {
        Some(ids) => ids,
        None => store.last_two()?.ok_or(
            "Need two snapshots of the same account and active role to compare. Run `flurry sync` again, or pass two ids.",
        )?,
    };
    let (table, incomplete) = store.diff(from, to)?;
    if let Some(m) = store.scope_mismatch(from, to)? {
        eprintln!(
            "WARNING: snapshots {from} and {to} differ ({m}), so the changes include what each login could see."
        );
    }
    if !incomplete.is_empty() {
        eprintln!(
            "UNKNOWN / INCOMPLETE: {} failed to collect in snapshot {from} or {to}, so its changes may be wrong.",
            incomplete.join(", ")
        );
    }
    eprintln!("Changes from snapshot {from} to snapshot {to}:");
    print_table(&table, format);
    Ok(())
}

async fn connection_test(p: &Profile) -> Result<(), String> {
    let session = connect(p).await?;
    let id = session.query(collect::IDENTITY_SQL).await?;
    println!("{:<12} {}", "fingerprint", collect::fingerprint(&id));
    let r = session
        .query("SELECT CURRENT_USER() AS USER, CURRENT_ROLE() AS ROLE, CURRENT_WAREHOUSE() AS WAREHOUSE")
        .await?;
    for c in &r.cols {
        println!("{c:<12} {}", r.get(0, c).unwrap_or_default());
    }
    Ok(())
}

/// Resolves a typed name like Snowflake does: `"Name"` is taken as is, an unquoted name is upper-cased.
fn ident(name: &str) -> String {
    match name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
        Some(quoted) => quoted.replace("\"\"", "\""),
        None => name.to_uppercase(),
    }
}

fn chain(p: &flurry_graph::AccessPath) -> String {
    let roles = p.roles.join(" -> ");
    if p.implicit {
        format!("{roles} (implicit)")
    } else {
        roles
    }
}

/// One row per path (or one row without path), each carrying the verdict and the completeness note.
fn access_table(verdict: &str, paths: &[flurry_graph::AccessPath], note: Option<&str>) -> Table {
    let row = |path: String, implicit: String| {
        vec![
            verdict.to_string(),
            path,
            implicit,
            note.unwrap_or_default().to_string(),
        ]
    };
    let rows = if paths.is_empty() {
        vec![row(String::new(), String::new())]
    } else {
        paths
            .iter()
            .map(|p| row(chain(p), p.implicit.to_string()))
            .collect()
    };
    let headers = ["result", "path", "implicit", "note"]
        .map(String::from)
        .to_vec();
    Table { headers, rows }
}

fn access(user: &str, warehouse: &str, privilege: &str, format: Format) -> Result<(), String> {
    let (user, warehouse) = (&ident(user), &ident(warehouse));
    let store = Store::open()?;
    for (entity, name, label) in [
        (Entity::Users, user, "user"),
        (Entity::Warehouses, warehouse, "warehouse"),
    ] {
        if !store.has(entity, name)? {
            return Err(format!("{label} '{name}' is not in the latest snapshot"));
        }
    }
    let result = store.graph()?.access(user, warehouse, privilege);
    let (verdict, paths, note) = match &result {
        Access::Granted { paths, truncated } => (
            "ACCESS",
            paths.as_slice(),
            truncated.then_some("more paths exist".to_string()),
        ),
        Access::NoKnownAccess => ("NO ACCESS", [].as_slice(), None),
        Access::Unknown { missing } => (
            "UNKNOWN / INCOMPLETE",
            [].as_slice(),
            Some(format!("not collected: {}", missing.join(", "))),
        ),
    };
    match format {
        Format::Csv => print_table(&access_table(verdict, paths, note.as_deref()), format),
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "user": user, "warehouse": warehouse, "privilege": privilege.to_uppercase(),
                "result": verdict, "note": note,
                "paths": paths
                    .iter()
                    .map(|p| json!({"implicit": p.implicit, "roles": p.roles}))
                    .collect::<Vec<_>>(),
            }))
            .unwrap_or_default()
        ),
        Format::Table => {
            println!(
                "{verdict}: {user} {} on {warehouse}",
                privilege.to_uppercase()
            );
            if let Some(n) = note {
                println!("  {n}");
            }
            for p in paths {
                println!("  {user} -> {}", chain(p));
            }
        }
    }
    Ok(())
}

fn inherits(role: &str, inverse: bool, format: Format) -> Result<(), String> {
    let role = ident(role);
    let store = Store::open()?;
    if !store.has(Entity::Roles, &role)? {
        return Err(format!("role '{role}' is not in the latest snapshot"));
    }
    let graph = store.graph()?;
    let roles = if inverse {
        graph.inheriting_roles(&role)
    } else {
        graph.inherited_roles(&role)
    };
    if !graph.role_edges_complete() {
        eprintln!(
            "UNKNOWN / INCOMPLETE: role_edges was not collected, so this list may be missing roles."
        );
    }
    let table = Table {
        headers: vec!["role".into()],
        rows: roles.into_iter().map(|r| vec![r]).collect(),
    };
    print_table(&table, format);
    Ok(())
}

fn list(entity: Entity, a: &ListArgs) -> Result<(), String> {
    print_table(&Store::open()?.list(entity, &a.search)?, a.format);
    Ok(())
}

fn tui() -> Result<(), String> {
    let store = Store::open()?;
    if store.list(Entity::Roles, "")?.rows.is_empty() {
        return Err("No roles in the local snapshot. Run `flurry sync` first.".into());
    }
    run_tui(&store, &store.graph()?).map_err(|e| e.to_string())
}

// Restores the terminal however the TUI loop exits, including on errors.
struct TermGuard;

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), crossterm::cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn run_tui(store: &Store, graph: &flurry_graph::Graph) -> std::io::Result<()> {
    terminal::enable_raw_mode()?;
    let _guard = TermGuard;
    execute!(stdout(), EnterAlternateScreen, crossterm::cursor::Hide)?;
    let mut s = State::new(Tab::Roles);
    let theme = ui::Theme::terminal();
    while !s.quit {
        let entity = match s.tab {
            Tab::Users => Entity::Users,
            Tab::Roles => Entity::Roles,
            Tab::Warehouses => Entity::Warehouses,
        };
        let table = store
            .list(entity, &s.filter)
            .map_err(std::io::Error::other)?;
        let items: Vec<String> = table.rows.iter().map(|r| r[0].clone()).collect();
        let cursor = s.cursor().min(items.len().saturating_sub(1));
        s.cursors[s.tab.index()] = cursor;
        let detail = match table.rows.get(cursor) {
            Some(row) => {
                detail::lines(store, graph, entity, &table, row).map_err(std::io::Error::other)?
            }
            None => Vec::new(),
        };
        let (w, h) = terminal::size()?;
        let out = ui::render(&items, &detail, &s, w as usize, h as usize, &theme);
        execute!(
            stdout(),
            crossterm::cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All)
        )?;
        print!("{out}");
        stdout().flush()?;
        let Event::Key(k) = event::read()? else {
            continue;
        };
        let key = match k.code {
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Tab => Key::Tab,
            KeyCode::BackTab => Key::BackTab,
            KeyCode::Esc => Key::Esc,
            KeyCode::Enter => Key::Enter,
            KeyCode::Backspace => Key::Backspace,
            _ => continue,
        };
        let size = if s.focus == Focus::Detail {
            detail.len()
        } else {
            items.len()
        };
        s = handle_key(s, key, size);
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let result = match Cli::parse().cmd {
        None => tui(),
        Some(Cmd::Sync(p)) => sync(&p).await,
        Some(Cmd::Refresh {
            kind,
            name,
            profile,
            ..
        }) => refresh(kind, name, &profile).await,
        Some(Cmd::Status) => status(),
        Some(Cmd::Snapshots { format }) => snapshots(format),
        Some(Cmd::Diff { from, to, format }) => diff(from.zip(to), format),
        Some(Cmd::Connection {
            action: ConnectionCmd::Test(p),
        }) => connection_test(&p).await,
        Some(Cmd::Access {
            user,
            warehouse,
            privilege,
            format,
        }) => access(&user, &warehouse, &privilege, format),
        Some(Cmd::Users {
            action: ListCmd::List(a),
        }) => list(Entity::Users, &a),
        Some(Cmd::Roles {
            action: RolesCmd::List(a),
        }) => list(Entity::Roles, &a),
        Some(Cmd::Roles {
            action:
                RolesCmd::Inherits {
                    role,
                    inverse,
                    format,
                },
        }) => inherits(&role, inverse, format),
        Some(Cmd::Warehouses {
            action: ListCmd::List(a),
        }) => list(Entity::Warehouses, &a),
    };
    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{access_table, csv_line, ident};
    use flurry_graph::AccessPath;

    #[test]
    fn ident_upper_cases_unquoted_names_only() {
        assert_eq!(ident("jane"), "JANE");
        assert_eq!(ident("\"Mixed\"\"Case\""), "Mixed\"Case");
    }

    #[test]
    fn csv_quotes_only_when_needed() {
        let cells = ["a".to_string(), "b,c".to_string(), "say \"hi\"".to_string()];
        assert_eq!(csv_line(&cells), "a,\"b,c\",\"say \"\"hi\"\"\"");
        assert_eq!(csv_line(&["x\ny".to_string()]), "\"x\ny\"");
    }

    #[test]
    fn access_table_keeps_the_completeness_note() {
        let t = access_table(
            "UNKNOWN / INCOMPLETE",
            &[],
            Some("not collected: role_edges"),
        );
        assert_eq!(
            t.rows,
            [["UNKNOWN / INCOMPLETE", "", "", "not collected: role_edges"]]
        );
        let paths = [AccessPath {
            implicit: false,
            roles: vec!["ANALYST".into(), "DATA_READ_ONLY".into()],
        }];
        let t = access_table("ACCESS", &paths, Some("more paths exist"));
        assert_eq!(
            t.rows,
            [[
                "ACCESS",
                "ANALYST -> DATA_READ_ONLY",
                "false",
                "more paths exist"
            ]]
        );
    }
}
