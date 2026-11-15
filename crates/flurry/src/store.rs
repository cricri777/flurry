use crate::collect::Snapshot;
use crate::config;
use rusqlite::{Connection, params};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS snapshots (
  id INTEGER PRIMARY KEY, profile TEXT NOT NULL, account TEXT NOT NULL,
  active_role TEXT NOT NULL, created_at INTEGER NOT NULL, fingerprint TEXT);
CREATE TABLE IF NOT EXISTS datasets (
  snapshot_id INTEGER NOT NULL, name TEXT NOT NULL, source TEXT NOT NULL,
  latency_minutes INTEGER, row_count INTEGER NOT NULL, collected_at INTEGER NOT NULL, error TEXT);
CREATE TABLE IF NOT EXISTS users (
  snapshot_id INTEGER NOT NULL, name TEXT NOT NULL, login_name TEXT,
  default_role TEXT, disabled TEXT, created_on TEXT);
CREATE TABLE IF NOT EXISTS roles (
  snapshot_id INTEGER NOT NULL, name TEXT NOT NULL, owner TEXT, created_on TEXT, comment TEXT);
CREATE TABLE IF NOT EXISTS warehouses (
  snapshot_id INTEGER NOT NULL, name TEXT NOT NULL, owner TEXT, type TEXT,
  size TEXT, created_on TEXT, comment TEXT);
CREATE TABLE IF NOT EXISTS user_roles (
  snapshot_id INTEGER NOT NULL, user_name TEXT NOT NULL, role_name TEXT NOT NULL,
  granted_by TEXT, created_on TEXT);
CREATE TABLE IF NOT EXISTS role_edges (
  snapshot_id INTEGER NOT NULL, parent TEXT NOT NULL, child TEXT NOT NULL,
  granted_by TEXT, created_on TEXT);
CREATE TABLE IF NOT EXISTS warehouse_privileges (
  snapshot_id INTEGER NOT NULL, role_name TEXT NOT NULL, warehouse TEXT NOT NULL,
  privilege TEXT NOT NULL, grant_option TEXT, granted_by TEXT);
CREATE INDEX IF NOT EXISTS i_users ON users (snapshot_id, name);
CREATE INDEX IF NOT EXISTS i_roles ON roles (snapshot_id, name);
CREATE INDEX IF NOT EXISTS i_warehouses ON warehouses (snapshot_id, name);
CREATE INDEX IF NOT EXISTS i_user_roles ON user_roles (snapshot_id, user_name);
CREATE INDEX IF NOT EXISTS i_role_edges ON role_edges (snapshot_id, parent);
CREATE INDEX IF NOT EXISTS i_wh_priv ON warehouse_privileges (snapshot_id, role_name);
";

pub struct Store(Connection);

/// A table of strings: column headers and rows.
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn e(err: rusqlite::Error) -> String {
    err.to_string()
}

// Adds columns introduced after the first release to existing databases.
fn migrate(conn: &Connection) -> Result<(), String> {
    let has: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('snapshots') WHERE name = 'fingerprint'",
            [],
            |r| r.get(0),
        )
        .map_err(e)?;
    if has == 0 {
        conn.execute("ALTER TABLE snapshots ADD COLUMN fingerprint TEXT", [])
            .map_err(e)?;
    }
    let has: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('datasets') WHERE name = 'error'",
            [],
            |r| r.get(0),
        )
        .map_err(e)?;
    if has == 0 {
        conn.execute("ALTER TABLE datasets ADD COLUMN error TEXT", [])
            .map_err(e)?;
    }
    Ok(())
}

fn ts(col: &str) -> String {
    format!("datetime({col}, 'unixepoch')")
}

// Inserts every row of a snapshot under the given snapshot id.
fn insert_rows(tx: &rusqlite::Transaction, id: i64, s: &Snapshot) -> Result<(), String> {
    for d in &s.datasets {
        tx.execute(
            "INSERT INTO datasets (snapshot_id, name, source, latency_minutes, row_count, collected_at, error) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                id,
                d.name,
                d.source,
                d.latency_minutes,
                d.rows as i64,
                d.collected_at as i64,
                d.error
            ],
        )
        .map_err(e)?;
    }
    for u in &s.users {
        tx.execute(
            "INSERT INTO users VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id,
                u.name,
                u.login_name,
                u.default_role,
                u.disabled,
                u.created_on
            ],
        )
        .map_err(e)?;
    }
    for r in &s.roles {
        tx.execute(
            "INSERT INTO roles VALUES (?, ?, ?, ?, ?)",
            params![id, r.name, r.owner, r.created_on, r.comment],
        )
        .map_err(e)?;
    }
    for w in &s.warehouses {
        tx.execute(
            "INSERT INTO warehouses VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![id, w.name, w.owner, w.kind, w.size, w.created_on, w.comment],
        )
        .map_err(e)?;
    }
    for g in &s.user_roles {
        tx.execute(
            "INSERT INTO user_roles VALUES (?, ?, ?, ?, ?)",
            params![id, g.user, g.role, g.granted_by, g.created_on],
        )
        .map_err(e)?;
    }
    for g in &s.role_edges {
        tx.execute(
            "INSERT INTO role_edges VALUES (?, ?, ?, ?, ?)",
            params![id, g.parent, g.child, g.granted_by, g.created_on],
        )
        .map_err(e)?;
    }
    for g in &s.warehouse_privileges {
        tx.execute(
            "INSERT INTO warehouse_privileges VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id,
                g.role,
                g.warehouse,
                g.privilege,
                g.grant_option,
                g.granted_by
            ],
        )
        .map_err(e)?;
    }
    Ok(())
}

impl Store {
    pub fn open() -> Result<Self, String> {
        let dir = config::data_dir().ok_or("HOME not set")?;
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join("flurry.db");
        let conn = Connection::open(&path).map_err(e)?;
        config::restrict(&dir, 0o700);
        config::restrict(&path, 0o600);
        conn.execute_batch(SCHEMA).map_err(e)?;
        migrate(&conn)?;
        Ok(Self(conn))
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        Self(conn)
    }

    /// Writes a snapshot in one transaction, so a failure keeps the previous snapshot intact.
    pub fn save(&mut self, profile: &str, s: &Snapshot) -> Result<i64, String> {
        let tx = self.0.transaction().map_err(e)?;
        tx.execute(
            "INSERT INTO snapshots (profile, account, active_role, created_at, fingerprint) VALUES (?, ?, ?, strftime('%s','now'), ?)",
            params![profile, s.account, s.active_role, s.fingerprint],
        )
        .map_err(e)?;
        let id = tx.last_insert_rowid();
        insert_rows(&tx, id, s)?;
        tx.commit().map_err(e)?;
        Ok(id)
    }

    /// Replaces one entity and its grants in the latest snapshot with freshly collected rows.
    pub fn refresh(&mut self, entity: Entity, name: &str, s: &Snapshot) -> Result<i64, String> {
        let tx = self.0.transaction().map_err(e)?;
        let id: i64 = tx
            .query_row("SELECT MAX(id) FROM snapshots", [], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .map_err(e)?
            .ok_or("No snapshot yet. Run `flurry sync` first.")?;
        let deletes: &[&str] = match entity {
            Entity::Users => &[
                "DELETE FROM users WHERE snapshot_id = ?1 AND name = ?2",
                "DELETE FROM user_roles WHERE snapshot_id = ?1 AND user_name = ?2",
            ],
            Entity::Roles => &[
                "DELETE FROM roles WHERE snapshot_id = ?1 AND name = ?2",
                "DELETE FROM role_edges WHERE snapshot_id = ?1 AND (parent = ?2 OR child = ?2)",
                "DELETE FROM user_roles WHERE snapshot_id = ?1 AND role_name = ?2",
                "DELETE FROM warehouse_privileges WHERE snapshot_id = ?1 AND role_name = ?2",
            ],
            Entity::Warehouses => &[
                "DELETE FROM warehouses WHERE snapshot_id = ?1 AND name = ?2",
                "DELETE FROM warehouse_privileges WHERE snapshot_id = ?1 AND warehouse = ?2",
            ],
        };
        for sql in deletes {
            tx.execute(sql, params![id, name]).map_err(e)?;
        }
        for d in &s.datasets {
            tx.execute(
                "DELETE FROM datasets WHERE snapshot_id = ? AND name = ?",
                params![id, d.name],
            )
            .map_err(e)?;
        }
        insert_rows(&tx, id, s)?;
        tx.commit().map_err(e)?;
        Ok(id)
    }

    /// Replaces one dataset row of the latest snapshot.
    pub fn record(&mut self, d: crate::collect::Dataset) -> Result<(), String> {
        let tx = self.0.transaction().map_err(e)?;
        let id: i64 = tx
            .query_row("SELECT MAX(id) FROM snapshots", [], |r| r.get(0))
            .map_err(e)?;
        tx.execute(
            "DELETE FROM datasets WHERE snapshot_id = ? AND name = ?",
            params![id, d.name],
        )
        .map_err(e)?;
        let snap = crate::collect::Snapshot {
            datasets: vec![d],
            ..Default::default()
        };
        insert_rows(&tx, id, &snap)?;
        tx.commit().map_err(e)
    }

    fn table(&self, sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Table, String> {
        let mut stmt = self.0.prepare(sql).map_err(e)?;
        let headers: Vec<String> = stmt.column_names().map(|c| c.to_string()).collect();
        let n = headers.len();
        let rows = stmt
            .query_map(args, |r| {
                (0..n)
                    .map(|i| {
                        Ok(match r.get_ref(i)? {
                            rusqlite::types::ValueRef::Null => String::new(),
                            rusqlite::types::ValueRef::Integer(v) => v.to_string(),
                            rusqlite::types::ValueRef::Real(v) => v.to_string(),
                            rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into(),
                            rusqlite::types::ValueRef::Blob(_) => String::new(),
                        })
                    })
                    .collect()
            })
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        Ok(Table { headers, rows })
    }

    /// Lists an entity from the latest snapshot, filtered by a case-insensitive name search.
    pub fn list(&self, entity: Entity, search: &str) -> Result<Table, String> {
        let (cols, table, name_col) = match entity {
            Entity::Users => (
                "name, login_name, default_role, disabled, created_on",
                "users",
                "name",
            ),
            Entity::Roles => ("name, owner, created_on, comment", "roles", "name"),
            Entity::Warehouses => ("name, type, size, owner, created_on", "warehouses", "name"),
        };
        let sql = format!(
            "SELECT {cols} FROM {table} WHERE snapshot_id = (SELECT MAX(id) FROM snapshots) AND {name_col} LIKE ? ESCAPE '\\' ORDER BY {name_col}"
        );
        let pattern = search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        self.table(&sql, &[&format!("%{pattern}%")])
    }

    /// Builds the RBAC graph of the latest snapshot, flagging failed datasets as incomplete.
    pub fn graph(&self) -> Result<flurry_graph::Graph, String> {
        let latest = "snapshot_id = (SELECT MAX(id) FROM snapshots)";
        let mut g = flurry_graph::Graph::default();
        for r in self
            .table(
                &format!("SELECT user_name, role_name FROM user_roles WHERE {latest}"),
                &[],
            )?
            .rows
        {
            g.add_user_role(&r[0], &r[1]);
        }
        for r in self
            .table(
                &format!("SELECT parent, child FROM role_edges WHERE {latest}"),
                &[],
            )?
            .rows
        {
            g.add_role_edge(&r[0], &r[1]);
        }
        let sql = format!(
            "SELECT role_name, warehouse, privilege FROM warehouse_privileges WHERE {latest}"
        );
        for r in self.table(&sql, &[])?.rows {
            g.add_warehouse_privilege(&r[0], &r[1], &r[2]);
        }
        let sql = format!("SELECT name FROM datasets WHERE {latest} AND error IS NOT NULL");
        for r in self.table(&sql, &[])?.rows {
            g.mark_incomplete(&r[0]);
        }
        Ok(g)
    }

    /// True when the latest snapshot has an entity with exactly this name.
    pub fn has(&self, entity: Entity, name: &str) -> Result<bool, String> {
        Ok(self.list(entity, name)?.rows.iter().any(|r| r[0] == name))
    }

    /// Latest snapshot header line, or None when nothing was synced yet.
    pub fn latest(&self) -> Result<Option<Table>, String> {
        let sql = format!(
            "SELECT id, profile, account, active_role, {} AS taken_at_utc, COALESCE(fingerprint, '') AS fingerprint FROM snapshots ORDER BY id DESC LIMIT 1",
            ts("created_at")
        );
        let t = self.table(&sql, &[])?;
        Ok((!t.rows.is_empty()).then_some(t))
    }

    pub fn datasets(&self) -> Result<Table, String> {
        let sql = format!(
            "SELECT name, source, row_count, {} AS collected_at_utc, COALESCE('up to ' || latency_minutes || ' min delayed', 'current') AS freshness, COALESCE('FAILED: ' || error, 'ok') AS status FROM datasets WHERE snapshot_id = (SELECT MAX(id) FROM snapshots) ORDER BY rowid",
            ts("collected_at")
        );
        self.table(&sql, &[])
    }
}

#[derive(Clone, Copy)]
pub enum Entity {
    Users,
    Roles,
    Warehouses,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::{Dataset, Role, RoleEdge, User, Warehouse};

    fn snapshot(roles: &[&str]) -> Snapshot {
        Snapshot {
            account: "ACME".into(),
            active_role: "ANALYST".into(),
            users: vec![User {
                name: "JANE".into(),
                login_name: None,
                default_role: Some("ANALYST".into()),
                disabled: Some("false".into()),
                created_on: None,
            }],
            roles: roles
                .iter()
                .map(|n| Role {
                    name: n.to_string(),
                    owner: Some("SECURITYADMIN".into()),
                    created_on: None,
                    comment: None,
                })
                .collect(),
            warehouses: vec![Warehouse {
                name: "WH_A".into(),
                owner: None,
                kind: Some("STANDARD".into()),
                size: Some("X-Small".into()),
                created_on: None,
                comment: None,
            }],
            role_edges: vec![RoleEdge {
                parent: "ANALYST".into(),
                child: "DATA_READ_ONLY".into(),
                granted_by: None,
                created_on: None,
            }],
            datasets: vec![
                Dataset {
                    name: "roles".into(),
                    source: "SHOW ROLES",
                    latency_minutes: None,
                    rows: roles.len(),
                    collected_at: 1_700_000_000,
                    error: None,
                },
                Dataset {
                    name: "role_edges".into(),
                    source: "ACCOUNT_USAGE.GRANTS_TO_ROLES",
                    latency_minutes: Some(120),
                    rows: 1,
                    collected_at: 1_700_000_000,
                    error: None,
                },
            ],
            ..Snapshot::default()
        }
    }

    #[test]
    fn no_snapshot_means_empty_lists_and_no_latest() {
        let s = Store::in_memory();
        assert!(s.latest().unwrap().is_none());
        assert!(s.list(Entity::Roles, "").unwrap().rows.is_empty());
    }

    #[test]
    fn saved_snapshot_is_listed_with_null_as_empty() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["ANALYST", "DATA_READ_ONLY"]))
            .unwrap();
        let roles = s.list(Entity::Roles, "").unwrap();
        assert_eq!(roles.headers[0], "name");
        assert_eq!(
            roles.rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(),
            ["ANALYST", "DATA_READ_ONLY"]
        );
        assert_eq!(roles.rows[0][3], "");
        assert_eq!(s.list(Entity::Users, "").unwrap().rows[0][0], "JANE");
        assert_eq!(
            s.list(Entity::Warehouses, "").unwrap().rows[0][2],
            "X-Small"
        );
    }

    #[test]
    fn search_is_case_insensitive_substring() {
        let mut s = Store::in_memory();
        s.save(
            "default",
            &snapshot(&["ANALYST", "DATA_READ_ONLY", "SYSADMIN"]),
        )
        .unwrap();
        let hits = s.list(Entity::Roles, "read").unwrap();
        assert_eq!(hits.rows.len(), 1);
        assert_eq!(hits.rows[0][0], "DATA_READ_ONLY");
        assert!(s.list(Entity::Roles, "zzz").unwrap().rows.is_empty());
    }

    #[test]
    fn lists_only_show_the_latest_snapshot() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["OLD_ROLE"])).unwrap();
        let second = s.save("default", &snapshot(&["NEW_ROLE"])).unwrap();
        let roles = s.list(Entity::Roles, "").unwrap();
        assert_eq!(roles.rows.len(), 1);
        assert_eq!(roles.rows[0][0], "NEW_ROLE");
        assert_eq!(s.latest().unwrap().unwrap().rows[0][0], second.to_string());
    }

    #[test]
    fn datasets_report_source_and_freshness() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["ANALYST"])).unwrap();
        let d = s.datasets().unwrap();
        assert_eq!(d.rows.len(), 2);
        assert_eq!(d.rows[0][4], "current");
        assert_eq!(d.rows[1][4], "up to 120 min delayed");
        assert_eq!(d.rows[1][3], "2023-11-14 22:13:20");
    }

    #[test]
    fn graph_reflects_the_latest_snapshot_and_failed_datasets() {
        let mut s = Store::in_memory();
        let mut snap = snapshot(&["ANALYST"]);
        snap.datasets[1].name = "role_edges".into();
        snap.datasets[1].error = Some("no access".into());
        s.save("default", &snap).unwrap();
        let g = s.graph().unwrap();
        assert_eq!(
            g.access("ALICE", "WH", "USAGE"),
            flurry_graph::Access::Unknown {
                missing: vec!["role_edges".into()]
            }
        );
        assert!(s.has(Entity::Roles, "ANALYST").unwrap());
        assert!(!s.has(Entity::Roles, "ANALY").unwrap());
    }

    #[test]
    fn failed_dataset_is_reported_with_its_error() {
        let mut s = Store::in_memory();
        let mut snap = snapshot(&["ANALYST"]);
        snap.datasets[1].error = Some("no access".into());
        s.save("default", &snap).unwrap();
        let d = s.datasets().unwrap();
        assert_eq!(d.rows[0][5], "ok");
        assert_eq!(d.rows[1][5], "FAILED: no access");
    }

    #[test]
    fn migration_adds_error_column_to_old_databases() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE snapshots (id INTEGER PRIMARY KEY, fingerprint TEXT);
             CREATE TABLE datasets (snapshot_id INTEGER, name TEXT);",
        )
        .unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        conn.execute("UPDATE datasets SET error = NULL", [])
            .unwrap();
    }

    #[test]
    fn record_replaces_one_dataset_row() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["ANALYST"])).unwrap();
        for rows in [3, 5] {
            s.record(Dataset {
                name: "refresh all roles".into(),
                source: "SHOW GRANTS TO/OF ROLE",
                latency_minutes: None,
                rows,
                collected_at: 1_700_000_000,
                error: None,
            })
            .unwrap();
        }
        let d = s.datasets().unwrap();
        let rows: Vec<_> = d
            .rows
            .iter()
            .filter(|r| r[0] == "refresh all roles")
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][2], "5");
    }

    #[test]
    fn like_wildcards_in_search_match_literally() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["A_B", "AXB", "50%"]))
            .unwrap();
        let hits = s.list(Entity::Roles, "A_B").unwrap();
        assert_eq!(hits.rows.len(), 1);
        assert_eq!(hits.rows[0][0], "A_B");
        assert_eq!(s.list(Entity::Roles, "%").unwrap().rows.len(), 1);
    }

    #[test]
    fn refresh_replaces_one_role_and_its_grants_only() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["ANALYST", "OTHER"])).unwrap();
        let fresh = Snapshot {
            roles: vec![Role {
                name: "ANALYST".into(),
                owner: Some("NEW_OWNER".into()),
                created_on: None,
                comment: None,
            }],
            datasets: vec![Dataset {
                name: "refresh roles ANALYST".into(),
                source: "SHOW GRANTS TO/OF ROLE",
                latency_minutes: None,
                rows: 0,
                collected_at: 1_700_000_000,
                error: None,
            }],
            ..Snapshot::default()
        };
        s.refresh(Entity::Roles, "ANALYST", &fresh).unwrap();
        let roles = s.list(Entity::Roles, "").unwrap();
        assert_eq!(roles.rows.len(), 2);
        assert_eq!(roles.rows[0][1], "NEW_OWNER");
        let edges: i64 =
            s.0.query_row("SELECT COUNT(*) FROM role_edges", [], |r| r.get(0))
                .unwrap();
        assert_eq!(edges, 0);
        assert_eq!(s.datasets().unwrap().rows.len(), 3);
    }

    #[test]
    fn refresh_without_snapshot_fails() {
        let mut s = Store::in_memory();
        assert!(
            s.refresh(Entity::Users, "JANE", &Snapshot::default())
                .is_err()
        );
    }
}
