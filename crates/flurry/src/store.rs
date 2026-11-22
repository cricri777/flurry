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

const SNAPSHOT_TABLES: [&str; 7] = [
    "datasets",
    "users",
    "roles",
    "warehouses",
    "user_roles",
    "role_edges",
    "warehouse_privileges",
];

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
    pub fn in_memory() -> Self {
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

    /// Deletes the given snapshots (all when `None`) in one transaction, failing on an unknown id.
    pub fn delete_snapshots(&mut self, ids: Option<&[i64]>) -> Result<usize, String> {
        let tx = self.0.transaction().map_err(e)?;
        let existing: Vec<i64> = tx
            .prepare("SELECT id FROM snapshots")
            .map_err(e)?
            .query_map([], |r| r.get(0))
            .map_err(e)?
            .collect::<Result<_, _>>()
            .map_err(e)?;
        let doomed: Vec<i64> = match ids {
            None => existing,
            Some(ids) => {
                if let Some(missing) = ids.iter().find(|id| !existing.contains(id)) {
                    return Err(format!("No snapshot with id {missing}."));
                }
                let mut unique = ids.to_vec();
                unique.sort_unstable();
                unique.dedup();
                unique
            }
        };
        for id in &doomed {
            for table in SNAPSHOT_TABLES {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE snapshot_id = ?"),
                    params![id],
                )
                .map_err(e)?;
            }
            tx.execute("DELETE FROM snapshots WHERE id = ?", params![id])
                .map_err(e)?;
        }
        tx.commit().map_err(e)?;
        Ok(doomed.len())
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

    /// Directly related names of one entity in the latest snapshot, as (label, values) sections.
    pub fn related(&self, entity: Entity, name: &str) -> Result<Vec<Related>, String> {
        let latest = "snapshot_id = (SELECT MAX(id) FROM snapshots)";
        let queries: &[(&str, Entity, &str)] = match entity {
            Entity::Users => &[(
                "roles",
                Entity::Roles,
                "SELECT role_name, role_name FROM user_roles WHERE user_name = ?",
            )],
            Entity::Roles => &[
                (
                    "users",
                    Entity::Users,
                    "SELECT user_name, user_name FROM user_roles WHERE role_name = ?",
                ),
                (
                    "warehouse privileges",
                    Entity::Warehouses,
                    "SELECT privilege || ' on ' || warehouse, warehouse FROM warehouse_privileges WHERE role_name = ?",
                ),
            ],
            Entity::Warehouses => &[(
                "granted to",
                Entity::Roles,
                "SELECT privilege || ' to ' || role_name, role_name FROM warehouse_privileges WHERE warehouse = ?",
            )],
        };
        queries
            .iter()
            .map(|&(label, target, sql)| {
                let sql = format!("{sql} AND {latest} ORDER BY 1");
                let rows = self.table(&sql, &[&name])?.rows;
                let items = rows
                    .into_iter()
                    .map(|mut r| (r.swap_remove(0), r.swap_remove(0)))
                    .collect();
                Ok(Related {
                    label,
                    target,
                    items,
                })
            })
            .collect()
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

    /// Every snapshot, newest first, with the number of datasets that failed to collect.
    pub fn snapshots(&self) -> Result<Table, String> {
        let sql = format!(
            "SELECT id, profile, account, active_role, {} AS taken_at_utc, (SELECT COUNT(*) FROM datasets d WHERE d.snapshot_id = s.id AND d.error IS NOT NULL) AS failed_datasets FROM snapshots s ORDER BY id DESC",
            ts("created_at")
        );
        self.table(&sql, &[])
    }

    /// Ids of the newest snapshot and the previous one with the same account and active role.
    pub fn last_two(&self) -> Result<Option<(i64, i64)>, String> {
        let t = self.table(
            "SELECT p.id, n.id FROM snapshots n JOIN snapshots p ON p.account = n.account AND p.active_role = n.active_role AND p.id < n.id WHERE n.id = (SELECT MAX(id) FROM snapshots) ORDER BY p.id DESC LIMIT 1",
            &[],
        )?;
        let id = |s: &str| s.parse::<i64>().map_err(|_| "bad snapshot id".to_string());
        t.rows
            .first()
            .map(|r| Ok((id(&r[0])?, id(&r[1])?)))
            .transpose()
    }

    /// Describes how two snapshots differ in account or active role, which makes their diff misleading.
    pub fn scope_mismatch(&self, from: i64, to: i64) -> Result<Option<String>, String> {
        let t = self.table(
            "SELECT a.account, b.account, a.active_role, b.active_role FROM snapshots a, snapshots b WHERE a.id = ?1 AND b.id = ?2",
            &[&from, &to],
        )?;
        let Some(r) = t.rows.first() else {
            return Ok(None);
        };
        let parts: Vec<String> = [("account", 0), ("active role", 2)]
            .into_iter()
            .filter(|&(_, i)| r[i] != r[i + 1])
            .map(|(label, i)| format!("{label} {} vs {}", r[i], r[i + 1]))
            .collect();
        Ok((!parts.is_empty()).then(|| parts.join(", ")))
    }

    /// Items added to or removed from `from` to reach `to`, plus the kinds whose dataset failed in either.
    pub fn diff(&self, from: i64, to: i64) -> Result<(Table, Vec<String>), String> {
        for id in [from, to] {
            if self
                .table("SELECT 1 FROM snapshots WHERE id = ?", &[&id])?
                .rows
                .is_empty()
            {
                return Err(format!("snapshot {id} does not exist"));
            }
        }
        let kinds = [
            ("users", "name"),
            ("roles", "name"),
            ("warehouses", "name"),
            ("user_roles", "user_name || ' -> ' || role_name"),
            ("role_edges", "parent || ' inherits ' || child"),
            (
                "warehouse_privileges",
                "role_name || ' ' || privilege || ' ON ' || warehouse",
            ),
        ];
        let mut rows = Vec::new();
        let mut incomplete = Vec::new();
        for (kind, item) in kinds {
            for (change, a, b) in [("added", from, to), ("removed", to, from)] {
                let sql = format!(
                    "SELECT {item} FROM {kind} WHERE snapshot_id = ?1 EXCEPT SELECT {item} FROM {kind} WHERE snapshot_id = ?2 ORDER BY 1"
                );
                for r in self.table(&sql, &[&b, &a])?.rows {
                    rows.push(vec![kind.to_string(), change.to_string(), r[0].clone()]);
                }
            }
            let failed = self.table(
                "SELECT 1 FROM datasets WHERE name = ?1 AND error IS NOT NULL AND snapshot_id IN (?2, ?3)",
                &[&kind, &from, &to],
            )?;
            if !failed.rows.is_empty() {
                incomplete.push(kind.to_string());
            }
        }
        let headers = ["kind", "change", "item"].map(String::from).to_vec();
        Ok((Table { headers, rows }, incomplete))
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

/// A detail section: its label, the kind of entity it points to, and (displayed text, target name) pairs.
pub struct Related {
    pub label: &'static str,
    pub target: Entity,
    pub items: Vec<(String, String)>,
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
    fn snapshots_are_listed_newest_first_with_failed_dataset_count() {
        let mut s = Store::in_memory();
        assert!(s.snapshots().unwrap().rows.is_empty());
        s.save("default", &snapshot(&["ANALYST"])).unwrap();
        let mut failed = snapshot(&["ANALYST"]);
        failed.datasets[1].error = Some("no access".into());
        s.save("other", &failed).unwrap();
        let t = s.snapshots().unwrap();
        assert_eq!(t.rows[0][..2], ["2", "other"]);
        assert_eq!(t.rows[0][5], "1");
        assert_eq!(t.rows[1][..2], ["1", "default"]);
        assert_eq!(t.rows[1][5], "0");
    }

    #[test]
    fn diff_lists_added_and_removed_items_and_flags_failed_datasets() {
        let mut s = Store::in_memory();
        assert!(s.last_two().unwrap().is_none());
        s.save("default", &snapshot(&["ANALYST", "OLD_ROLE"]))
            .unwrap();
        let mut next = snapshot(&["ANALYST", "NEW_ROLE"]);
        next.role_edges.clear();
        next.datasets[1].error = Some("no access".into());
        s.save("default", &next).unwrap();
        assert_eq!(s.last_two().unwrap(), Some((1, 2)));
        let (t, incomplete) = s.diff(1, 2).unwrap();
        assert_eq!(
            t.rows,
            [
                ["roles", "added", "NEW_ROLE"],
                ["roles", "removed", "OLD_ROLE"],
                ["role_edges", "removed", "ANALYST inherits DATA_READ_ONLY"],
            ]
        );
        assert_eq!(incomplete, ["role_edges"]);
        assert!(s.diff(1, 1).unwrap().0.rows.is_empty());
        assert!(s.diff(1, 9).is_err());
    }

    #[test]
    fn default_diff_pairs_only_snapshots_of_the_same_account_and_role() {
        let mut s = Store::in_memory();
        s.save("prod", &snapshot(&["ANALYST"])).unwrap();
        assert!(s.last_two().unwrap().is_none());
        let mut other = snapshot(&["ANALYST"]);
        other.account = "OTHER".into();
        other.active_role = "PUBLIC".into();
        s.save("dev", &other).unwrap();
        assert!(s.last_two().unwrap().is_none());
        assert_eq!(
            s.scope_mismatch(1, 2).unwrap().as_deref(),
            Some("account ACME vs OTHER, active role ANALYST vs PUBLIC")
        );
        s.save("prod", &snapshot(&["ANALYST"])).unwrap();
        assert_eq!(s.last_two().unwrap(), Some((1, 3)));
        assert!(s.scope_mismatch(1, 3).unwrap().is_none());
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
    fn delete_snapshots_removes_only_the_given_ids() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["A"])).unwrap();
        s.save("default", &snapshot(&["B"])).unwrap();
        assert_eq!(s.delete_snapshots(Some(&[1])).unwrap(), 1);
        let left = s.snapshots().unwrap();
        assert_eq!(left.rows.len(), 1);
        assert_eq!(left.rows[0][0], "2");
        assert_eq!(s.list(Entity::Roles, "").unwrap().rows.len(), 1);
    }

    #[test]
    fn delete_snapshots_with_unknown_id_deletes_nothing() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["A"])).unwrap();
        assert!(s.delete_snapshots(Some(&[1, 99])).is_err());
        assert_eq!(s.snapshots().unwrap().rows.len(), 1);
    }

    #[test]
    fn delete_all_empties_every_snapshot_table() {
        let mut s = Store::in_memory();
        s.save("default", &snapshot(&["A"])).unwrap();
        s.save("default", &snapshot(&["B"])).unwrap();
        assert_eq!(s.delete_snapshots(None).unwrap(), 2);
        for table in SNAPSHOT_TABLES.iter().chain(&["snapshots"]) {
            let n: i64 =
                s.0.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                    .unwrap();
            assert_eq!(n, 0, "{table}");
        }
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
