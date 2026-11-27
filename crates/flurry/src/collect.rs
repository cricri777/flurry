use crate::session::{Rows, Session};
use crate::store::Entity;

type Field = Option<String>;

pub struct User {
    pub name: String,
    pub login_name: Field,
    pub default_role: Field,
    pub disabled: Field,
    pub created_on: Field,
}

pub struct Role {
    pub name: String,
    pub owner: Field,
    pub created_on: Field,
    pub comment: Field,
}

pub struct Warehouse {
    pub name: String,
    pub owner: Field,
    pub kind: Field,
    pub size: Field,
    pub created_on: Field,
    pub comment: Field,
}

pub struct UserRole {
    pub user: String,
    pub role: String,
    pub granted_by: Field,
    pub created_on: Field,
}

/// Parent role inherits the privileges of the child role.
pub struct RoleEdge {
    pub parent: String,
    pub child: String,
    pub granted_by: Field,
    pub created_on: Field,
}

pub struct WarehousePrivilege {
    pub role: String,
    pub warehouse: String,
    pub privilege: String,
    pub grant_option: Field,
    pub granted_by: Field,
}

/// Any privilege granted to a role; `name` is the fully qualified object name.
pub struct Grant {
    pub role: String,
    pub privilege: String,
    pub granted_on: String,
    pub name: String,
    pub grant_option: Field,
    pub granted_by: Field,
}

/// Provenance of one collected dataset.
pub struct Dataset {
    pub name: String,
    pub source: &'static str,
    pub latency_minutes: Option<u32>,
    pub rows: usize,
    pub collected_at: u64,
    /// Set when the dataset could not be collected; its rows are then unknown, not empty.
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Snapshot {
    pub account: String,
    pub fingerprint: String,
    pub active_role: String,
    pub users: Vec<User>,
    pub roles: Vec<Role>,
    pub warehouses: Vec<Warehouse>,
    pub user_roles: Vec<UserRole>,
    pub role_edges: Vec<RoleEdge>,
    pub warehouse_privileges: Vec<WarehousePrivilege>,
    pub grants: Vec<Grant>,
    pub datasets: Vec<Dataset>,
}

const PAGE: usize = 10_000;
const USAGE_LATENCY: u32 = 120;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

type Log<'a> = &'a (dyn Fn(&str) + Sync);

// Shortens a statement to fit a log line: drops the SELECT column list and the WHERE clause.
fn short(sql: &str) -> String {
    let sql = sql.split(" WHERE ").next().unwrap_or(sql);
    match sql.split_once(" FROM ") {
        Some((_, table)) if sql.starts_with("SELECT") => format!("SELECT ... FROM {table}"),
        _ => sql.to_string(),
    }
}

// Runs a statement, logging it and its row count.
async fn run(s: &Session, log: Log<'_>, sql: &str) -> Result<Rows, String> {
    log(&format!("> {}", short(sql)));
    let r = s.query(sql).await?;
    log(&format!("  {} rows", r.rows.len()));
    Ok(r)
}

// Pages through SHOW <object> with LIMIT/FROM until a short page is returned.
async fn show_paged(s: &Session, log: Log<'_>, object: &str) -> Result<Rows, String> {
    let mut all: Option<Rows> = None;
    let mut from = String::new();
    loop {
        let page = run(s, log, &format!("SHOW {object} LIMIT {PAGE}{from}")).await?;
        let n = page.rows.len();
        let last = page.get(n.wrapping_sub(1), "name");
        match &mut all {
            Some(a) => a.rows.extend(page.rows),
            None => all = Some(page),
        }
        match last {
            Some(name) if n == PAGE => from = format!(" FROM '{}'", name.replace('\'', "''")),
            _ => return Ok(all.expect("set above")),
        }
    }
}

fn dataset(name: &str, source: &'static str, latency: Option<u32>, rows: usize) -> Dataset {
    Dataset {
        name: name.into(),
        source,
        latency_minutes: latency,
        rows,
        collected_at: now(),
        error: None,
    }
}

fn failed(name: &str, source: &'static str, latency: Option<u32>, error: &str) -> Dataset {
    Dataset {
        error: Some(error.into()),
        ..dataset(name, source, latency, 0)
    }
}

pub const IDENTITY_SQL: &str = "SELECT CURRENT_ACCOUNT() AS ACCOUNT, CURRENT_ROLE() AS ROLE, CURRENT_ORGANIZATION_NAME() AS ORG, CURRENT_ACCOUNT_NAME() AS ACCOUNT_NAME, CURRENT_REGION() AS REGION";

/// Stable account identity (`ORG-ACCOUNT (REGION)`), falling back to the account locator.
pub fn fingerprint(id: &Rows) -> String {
    let get = |c| id.get(0, c).filter(|v| !v.is_empty());
    let name = match (get("org"), get("account_name")) {
        (Some(o), Some(a)) => format!("{o}-{a}"),
        _ => get("account").unwrap_or_default(),
    };
    match get("region") {
        Some(r) => format!("{name} ({r})"),
        None => name,
    }
}

/// Refuses to merge data from another account into a snapshot; legacy snapshots without a fingerprint are accepted.
pub fn ensure_same_account(stored: &str, current: &str) -> Result<(), String> {
    if stored.is_empty() || stored == current {
        return Ok(());
    }
    Err(format!(
        "The latest snapshot is from account {stored} but this login is {current}. Run `flurry sync` to take a new snapshot."
    ))
}

/// Quotes a Snowflake identifier so raw input can never alter the statement.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

// Keeps the rows for which `keep` holds.
fn keep(r: Rows, keep: impl Fn(&Rows, usize) -> bool) -> Rows {
    let idx: Vec<usize> = (0..r.rows.len()).filter(|&i| keep(&r, i)).collect();
    let rows = idx.iter().map(|&i| r.rows[i].clone()).collect();
    Rows { cols: r.cols, rows }
}

/// Error suffix for an entity that no longer exists or is hidden from the active role.
pub const NOT_FOUND: &str = "not found or not visible to the active role";

// SHOW ... LIKE is a case-insensitive pattern, so the exact name is matched afterwards.
async fn show_exact(s: &Session, object: &str, name: &str) -> Result<Rows, String> {
    let pattern = name.replace('\\', "\\\\").replace('\'', "''");
    let r = s.query(&format!("SHOW {object} LIKE '{pattern}'")).await?;
    let r = keep(r, |r, i| r.get(i, "name").as_deref() == Some(name));
    if r.rows.is_empty() {
        return Err(format!("{name}: {NOT_FOUND}"));
    }
    Ok(r)
}

/// Re-reads one entity and its grants with targeted SHOW commands; the result holds only the affected rows.
pub async fn refresh(s: &Session, entity: Entity, name: &str) -> Result<Snapshot, String> {
    let mut snap = Snapshot::default();
    let q = quote_ident(name);
    match entity {
        Entity::Users => {
            let r = show_exact(s, "USERS", name).await?;
            snap.users.push(user_at(&r, 0));
            let g = s.query(&format!("SHOW GRANTS TO USER {q}")).await?;
            snap.user_roles = user_grants(name, &g);
            snap.datasets.push(dataset(
                &format!("refresh users {name}"),
                "SHOW GRANTS TO USER",
                None,
                snap.user_roles.len(),
            ));
        }
        Entity::Roles => {
            let r = show_exact(s, "ROLES", name).await?;
            snap.roles.push(role_at(&r, 0));
            let to = s.query(&format!("SHOW GRANTS TO ROLE {q}")).await?;
            (snap.role_edges, snap.warehouse_privileges) = split_role_grants(&to);
            snap.grants = role_grants(&to);
            let of = s.query(&format!("SHOW GRANTS OF ROLE {q}")).await?;
            let (users, parents) = role_holders(name, &of);
            snap.user_roles = users;
            snap.role_edges.extend(parents);
            let rows = snap.user_roles.len() + snap.role_edges.len() + snap.grants.len();
            snap.datasets.push(dataset(
                &format!("refresh roles {name}"),
                "SHOW GRANTS TO/OF ROLE",
                None,
                rows,
            ));
        }
        Entity::Warehouses => {
            let r = show_exact(s, "WAREHOUSES", name).await?;
            snap.warehouses.push(warehouse_at(&r, 0));
            let g = s.query(&format!("SHOW GRANTS ON WAREHOUSE {q}")).await?;
            let g = keep(g, |r, i| {
                r.get(i, "granted_to").as_deref() == Some("ROLE")
                    && r.get(i, "granted_on").as_deref() == Some("WAREHOUSE")
            });
            (_, snap.warehouse_privileges) = split_role_grants(&g);
            snap.datasets.push(dataset(
                &format!("refresh warehouses {name}"),
                "SHOW GRANTS ON WAREHOUSE",
                None,
                snap.warehouse_privileges.len(),
            ));
        }
    }
    Ok(snap)
}

// SHOW GRANTS TO USER lists the roles in the `role` column.
fn user_grants(user: &str, r: &Rows) -> Vec<UserRole> {
    (0..r.rows.len())
        .map(|i| UserRole {
            user: user.into(),
            role: r.get(i, "role").unwrap_or_default(),
            granted_by: r.get(i, "granted_by"),
            created_on: r.get(i, "created_on"),
        })
        .collect()
}

// SHOW GRANTS OF ROLE lists who holds the role: users and parent roles that inherit it.
fn role_holders(role: &str, r: &Rows) -> (Vec<UserRole>, Vec<RoleEdge>) {
    let (mut users, mut parents) = (Vec::new(), Vec::new());
    for i in 0..r.rows.len() {
        let grantee = r.get(i, "grantee_name").unwrap_or_default();
        let (granted_by, created_on) = (r.get(i, "granted_by"), r.get(i, "created_on"));
        match r.get(i, "granted_to").as_deref() {
            Some("USER") => users.push(UserRole {
                user: grantee,
                role: role.into(),
                granted_by,
                created_on,
            }),
            Some("ROLE") => parents.push(RoleEdge {
                parent: grantee,
                child: role.into(),
                granted_by,
                created_on,
            }),
            _ => {}
        }
    }
    (users, parents)
}

fn user_at(r: &Rows, i: usize) -> User {
    User {
        name: r.get(i, "name").unwrap_or_default(),
        login_name: r.get(i, "login_name"),
        default_role: r.get(i, "default_role"),
        disabled: r.get(i, "disabled"),
        created_on: r.get(i, "created_on"),
    }
}

fn role_at(r: &Rows, i: usize) -> Role {
    Role {
        name: r.get(i, "name").unwrap_or_default(),
        owner: r.get(i, "owner"),
        created_on: r.get(i, "created_on"),
        comment: r.get(i, "comment"),
    }
}

fn warehouse_at(r: &Rows, i: usize) -> Warehouse {
    Warehouse {
        name: r.get(i, "name").unwrap_or_default(),
        owner: r.get(i, "owner"),
        kind: r.get(i, "type"),
        size: r.get(i, "size"),
        created_on: r.get(i, "created_on"),
        comment: r.get(i, "comment"),
    }
}

/// Ok with the message when access was denied, so the dataset is recorded as failed; Err for any other error, which aborts.
fn access_denied(e: String) -> Result<String, String> {
    let lower = e.to_lowercase();
    let msg = format!("{e} (the active role needs access to SNOWFLAKE.ACCOUNT_USAGE)");
    if lower.contains("does not exist or not authorized")
        || lower.contains("insufficient privileges")
    {
        Ok(msg)
    } else {
        Err(msg)
    }
}

/// Splits GRANTS_TO_ROLES rows into role inheritance edges and warehouse privileges, ignoring other grants.
fn split_role_grants(r: &Rows) -> (Vec<RoleEdge>, Vec<WarehousePrivilege>) {
    let (mut edges, mut privileges) = (Vec::new(), Vec::new());
    for i in 0..r.rows.len() {
        let name = r.get(i, "name").unwrap_or_default();
        let grantee = r.get(i, "grantee_name").unwrap_or_default();
        let on = r.get(i, "granted_on");
        if on.as_deref() == Some("ROLE") && r.get(i, "privilege").as_deref() == Some("USAGE") {
            // GRANT ROLE <name> TO ROLE <grantee>: the grantee (parent) inherits the named (child) role.
            edges.push(RoleEdge {
                parent: grantee,
                child: name,
                granted_by: r.get(i, "granted_by"),
                created_on: r.get(i, "created_on"),
            });
        } else if on.as_deref() == Some("WAREHOUSE") {
            privileges.push(WarehousePrivilege {
                role: grantee,
                warehouse: name,
                privilege: r.get(i, "privilege").unwrap_or_default(),
                grant_option: r.get(i, "grant_option"),
                granted_by: r.get(i, "granted_by"),
            });
        }
    }
    (edges, privileges)
}

// ACCOUNT_USAGE splits the qualified name into TABLE_CATALOG and TABLE_SCHEMA; SHOW GRANTS already qualifies NAME.
fn role_grants(r: &Rows) -> Vec<Grant> {
    (0..r.rows.len())
        .map(|i| {
            let on = r.get(i, "granted_on").unwrap_or_default();
            let name = r.get(i, "name").unwrap_or_default();
            let scope = match on.as_str() {
                "DATABASE" => vec![],
                "SCHEMA" => vec![r.get(i, "table_catalog")],
                _ => vec![r.get(i, "table_catalog"), r.get(i, "table_schema")],
            };
            let mut parts: Vec<String> = scope
                .into_iter()
                .flatten()
                .filter(|p| !p.is_empty())
                .collect();
            parts.push(name);
            Grant {
                role: r.get(i, "grantee_name").unwrap_or_default(),
                privilege: r.get(i, "privilege").unwrap_or_default(),
                granted_on: on,
                name: parts.join("."),
                grant_option: r.get(i, "grant_option"),
                granted_by: r.get(i, "granted_by"),
            }
        })
        .collect()
}

/// Number of "Collecting ..." steps `collect` logs, for the TUI progress bar.
pub const STEPS: usize = 5;

/// Collects a full metadata snapshot; access denied on an ACCOUNT_USAGE grant dataset records it as failed, any other error aborts.
pub async fn collect(s: &Session, log: Log<'_>) -> Result<Snapshot, String> {
    let mut snap = Snapshot::default();

    let id = run(s, log, IDENTITY_SQL).await?;
    snap.account = id.get(0, "account").unwrap_or_default();
    snap.active_role = id.get(0, "role").unwrap_or_default();
    snap.fingerprint = fingerprint(&id);

    log("Collecting users...");
    let r = show_paged(s, log, "USERS").await?;
    for i in 0..r.rows.len() {
        snap.users.push(user_at(&r, i));
    }
    snap.datasets
        .push(dataset("users", "SHOW USERS", None, snap.users.len()));

    log("Collecting roles...");
    let r = show_paged(s, log, "ROLES").await?;
    for i in 0..r.rows.len() {
        snap.roles.push(role_at(&r, i));
    }
    snap.datasets
        .push(dataset("roles", "SHOW ROLES", None, snap.roles.len()));

    log("Collecting warehouses...");
    let r = run(s, log, "SHOW WAREHOUSES").await?;
    for i in 0..r.rows.len() {
        snap.warehouses.push(warehouse_at(&r, i));
    }
    snap.datasets.push(dataset(
        "warehouses",
        "SHOW WAREHOUSES",
        None,
        snap.warehouses.len(),
    ));

    log("Collecting user-role grants (can take a few minutes)...");
    let src = "ACCOUNT_USAGE.GRANTS_TO_USERS";
    match run(s, log, "SELECT TO_VARCHAR(CREATED_ON) AS CREATED_ON, ROLE, GRANTEE_NAME, GRANTED_BY FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS WHERE DELETED_ON IS NULL")
        .await
    {
        Ok(r) => {
            for i in 0..r.rows.len() {
                snap.user_roles.push(UserRole {
                    user: r.get(i, "grantee_name").unwrap_or_default(),
                    role: r.get(i, "role").unwrap_or_default(),
                    granted_by: r.get(i, "granted_by"),
                    created_on: r.get(i, "created_on"),
                });
            }
            snap.datasets.push(dataset(
                "user_roles",
                src,
                Some(USAGE_LATENCY),
                snap.user_roles.len(),
            ));
        }
        Err(e) => snap.datasets.push(failed(
            "user_roles",
            src,
            Some(USAGE_LATENCY),
            &access_denied(e)?,
        )),
    }

    log("Collecting role grants (can take a few minutes)...");
    let src = "ACCOUNT_USAGE.GRANTS_TO_ROLES";
    match run(s, log, "SELECT TO_VARCHAR(CREATED_ON) AS CREATED_ON, PRIVILEGE, GRANTED_ON, TABLE_CATALOG, TABLE_SCHEMA, NAME, GRANTEE_NAME, GRANT_OPTION, GRANTED_BY FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES WHERE DELETED_ON IS NULL AND GRANTED_TO = 'ROLE'")
        .await
    {
        Ok(r) => {
            (snap.role_edges, snap.warehouse_privileges) = split_role_grants(&r);
            snap.grants = role_grants(&r);
            snap.datasets.push(dataset(
                "role_edges",
                src,
                Some(USAGE_LATENCY),
                snap.role_edges.len(),
            ));
            snap.datasets.push(dataset(
                "warehouse_privileges",
                src,
                Some(USAGE_LATENCY),
                snap.warehouse_privileges.len(),
            ));
            snap.datasets.push(dataset(
                "role_grants",
                src,
                Some(USAGE_LATENCY),
                snap.grants.len(),
            ));
        }
        Err(e) => {
            let msg = access_denied(e)?;
            for name in ["role_edges", "warehouse_privileges", "role_grants"] {
                snap.datasets
                    .push(failed(name, src, Some(USAGE_LATENCY), &msg));
            }
        }
    }
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_access_denied_is_recorded_as_failed() {
        assert!(
            access_denied(
                "SQL compilation error: Object 'X' does not exist or not authorized.".into()
            )
            .is_ok()
        );
        assert!(
            access_denied(
                "SQL access control error: Insufficient privileges to operate on schema".into()
            )
            .is_ok()
        );
        assert!(access_denied("Cannot reach Snowflake: timeout".into()).is_err());
    }

    fn rows<const N: usize>(cols: [&str; N], data: &[[&str; N]]) -> Rows {
        Rows {
            cols: cols.iter().map(|c| c.to_string()).collect(),
            rows: data
                .iter()
                .map(|r| r.iter().map(|c| Some(c.to_string())).collect())
                .collect(),
        }
    }

    #[test]
    fn child_role_grant_becomes_parent_inherits_child_edge() {
        let r = rows(
            [
                "privilege",
                "granted_on",
                "name",
                "grantee_name",
                "grant_option",
                "granted_by",
                "created_on",
            ],
            &[[
                "USAGE",
                "ROLE",
                "DATA_READ_ONLY",
                "ANALYST",
                "false",
                "SECURITYADMIN",
                "t1",
            ]],
        );
        let (edges, privileges) = split_role_grants(&r);
        assert!(privileges.is_empty());
        assert_eq!(
            (edges[0].parent.as_str(), edges[0].child.as_str()),
            ("ANALYST", "DATA_READ_ONLY")
        );
        assert_eq!(edges[0].granted_by.as_deref(), Some("SECURITYADMIN"));
    }

    #[test]
    fn warehouse_grant_becomes_privilege_of_the_grantee_role() {
        let r = rows(
            [
                "privilege",
                "granted_on",
                "name",
                "grantee_name",
                "grant_option",
                "granted_by",
                "created_on",
            ],
            &[[
                "MONITOR",
                "WAREHOUSE",
                "WH_A",
                "DATA_READ_ONLY",
                "true",
                "SYSADMIN",
                "t2",
            ]],
        );
        let (edges, privileges) = split_role_grants(&r);
        assert!(edges.is_empty());
        let p = &privileges[0];
        assert_eq!(
            (p.role.as_str(), p.warehouse.as_str(), p.privilege.as_str()),
            ("DATA_READ_ONLY", "WH_A", "MONITOR")
        );
        assert_eq!(p.grant_option.as_deref(), Some("true"));
    }

    #[test]
    fn mixed_rows_are_split_in_order() {
        let r = rows(
            ["privilege", "granted_on", "name", "grantee_name"],
            &[
                ["USAGE", "ROLE", "C1", "P1"],
                ["USAGE", "WAREHOUSE", "W1", "P1"],
                ["USAGE", "ROLE", "C2", "P1"],
            ],
        );
        let (edges, privileges) = split_role_grants(&r);
        assert_eq!(
            edges.iter().map(|e| e.child.as_str()).collect::<Vec<_>>(),
            ["C1", "C2"]
        );
        assert_eq!(privileges.len(), 1);
    }

    #[test]
    fn fingerprint_prefers_org_account_name_and_region() {
        let r = rows(
            ["account", "org", "account_name", "region"],
            &[["AB123", "ACME", "PROD", "AWS_US_EAST_1"]],
        );
        assert_eq!(fingerprint(&r), "ACME-PROD (AWS_US_EAST_1)");
        let r = rows(["account"], &[["AB123"]]);
        assert_eq!(fingerprint(&r), "AB123");
    }

    #[test]
    fn logged_statements_drop_the_column_list_and_filters() {
        assert_eq!(short("SHOW USERS LIMIT 10"), "SHOW USERS LIMIT 10");
        assert_eq!(
            short("SELECT A, B FROM SNOWFLAKE.ACCOUNT_USAGE.X WHERE DELETED_ON IS NULL"),
            "SELECT ... FROM SNOWFLAKE.ACCOUNT_USAGE.X"
        );
    }

    #[test]
    fn refresh_refuses_a_different_account() {
        assert!(ensure_same_account("ACME-PROD (R1)", "ACME-PROD (R1)").is_ok());
        assert!(ensure_same_account("", "ACME-PROD (R1)").is_ok());
        let err = ensure_same_account("ACME-PROD (R1)", "OTHER-DEV (R1)").unwrap_err();
        assert!(err.contains("ACME-PROD (R1)") && err.contains("OTHER-DEV (R1)"));
    }

    #[test]
    fn identifiers_are_quoted_and_escaped() {
        assert_eq!(quote_ident("ANALYST"), "\"ANALYST\"");
        assert_eq!(quote_ident("A\"; DROP"), "\"A\"\"; DROP\"");
    }

    #[test]
    fn role_holders_split_users_and_parent_roles() {
        let r = rows(
            ["granted_to", "grantee_name"],
            &[
                ["USER", "JANE"],
                ["ROLE", "SYSADMIN"],
                ["DATABASE_ROLE", "X"],
            ],
        );
        let (users, parents) = role_holders("ANALYST", &r);
        assert_eq!(users[0].user, "JANE");
        assert_eq!(users[0].role, "ANALYST");
        assert_eq!(parents.len(), 1);
        assert_eq!(
            (parents[0].parent.as_str(), parents[0].child.as_str()),
            ("SYSADMIN", "ANALYST")
        );
    }

    #[test]
    fn role_grants_keep_only_role_usage_and_warehouse_rows() {
        let r = rows(
            ["privilege", "granted_on", "name", "grantee_name"],
            &[
                ["USAGE", "ROLE", "C1", "P1"],
                ["OWNERSHIP", "ROLE", "C2", "P1"],
                ["SELECT", "TABLE", "T1", "P1"],
                ["USAGE", "WAREHOUSE", "W1", "P1"],
            ],
        );
        let (edges, privileges) = split_role_grants(&r);
        assert_eq!(edges.len(), 1);
        assert_eq!(privileges.len(), 1);
        assert_eq!(role_grants(&r).len(), 4);
    }

    #[test]
    fn grant_names_are_qualified_from_account_usage_columns() {
        let r = rows(
            [
                "privilege",
                "granted_on",
                "table_catalog",
                "table_schema",
                "name",
                "grantee_name",
            ],
            &[
                ["USAGE", "DATABASE", "DB", "", "DB", "R"],
                ["USAGE", "SCHEMA", "DB", "S", "S", "R"],
                ["SELECT", "TABLE", "DB", "S", "T", "R"],
                ["USAGE", "WAREHOUSE", "", "", "WH", "R"],
            ],
        );
        let names: Vec<String> = role_grants(&r).into_iter().map(|g| g.name).collect();
        assert_eq!(names, ["DB", "DB.S", "DB.S.T", "WH"]);
    }
}
