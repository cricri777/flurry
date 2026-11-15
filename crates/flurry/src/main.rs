mod collect;
mod config;
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
use modes::{Key, Mode, State, handle_key};
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
        action: ListCmd,
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
    let chain = |p: &flurry_graph::AccessPath| {
        let roles = p.roles.join(" -> ");
        if p.implicit {
            format!("{roles} (implicit)")
        } else {
            roles
        }
    };
    match format {
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

fn list(entity: Entity, a: &ListArgs) -> Result<(), String> {
    print_table(&Store::open()?.list(entity, &a.search)?, a.format);
    Ok(())
}

fn tui() -> Result<(), String> {
    let items: Vec<String> = Store::open()?
        .list(Entity::Roles, "")?
        .rows
        .into_iter()
        .map(|mut r| r.swap_remove(0))
        .collect();
    if items.is_empty() {
        return Err("No roles in the local snapshot. Run `flurry sync` first.".into());
    }
    run_tui(&items).map_err(|e| e.to_string())
}

// Restores the terminal however the TUI loop exits, including on errors.
struct TermGuard;

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), crossterm::cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn run_tui(items: &[String]) -> std::io::Result<()> {
    terminal::enable_raw_mode()?;
    let _guard = TermGuard;
    execute!(stdout(), EnterAlternateScreen, crossterm::cursor::Hide)?;
    let mut s = State {
        mode: Mode::Normal,
        cursor: 0,
        buffer: String::new(),
    };
    let theme = ui::Theme::terminal();
    loop {
        let (w, h) = terminal::size()?;
        let out = ui::render(items, &s, w as usize, h as usize, &theme);
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
            KeyCode::Char('q') if s.mode == Mode::Normal => break,
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Esc => Key::Esc,
            KeyCode::Enter => Key::Enter,
            KeyCode::Backspace => Key::Backspace,
            _ => continue,
        };
        s = handle_key(s, key, items.len());
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
            action: ListCmd::List(a),
        }) => list(Entity::Roles, &a),
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
    use super::ident;

    #[test]
    fn ident_upper_cases_unquoted_names_only() {
        assert_eq!(ident("jane"), "JANE");
        assert_eq!(ident("\"Mixed\"\"Case\""), "Mixed\"Case");
    }
}
