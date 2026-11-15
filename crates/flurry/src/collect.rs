use crate::session::{Rows, Session};

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

/// Provenance of one collected dataset.
pub struct Dataset {
    pub name: &'static str,
    pub source: &'static str,
    pub latency_minutes: Option<u32>,
    pub rows: usize,
    pub collected_at: u64,
}

#[derive(Default)]
pub struct Snapshot {
    pub account: String,
    pub active_role: String,
    pub users: Vec<User>,
    pub roles: Vec<Role>,
    pub warehouses: Vec<Warehouse>,
    pub user_roles: Vec<UserRole>,
    pub role_edges: Vec<RoleEdge>,
    pub warehouse_privileges: Vec<WarehousePrivilege>,
    pub datasets: Vec<Dataset>,
}

const PAGE: usize = 10_000;
const USAGE_LATENCY: u32 = 120;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// Pages through SHOW <object> with LIMIT/FROM until a short page is returned.
async fn show_paged(s: &Session, object: &str) -> Result<Rows, String> {
    let mut all: Option<Rows> = None;
    let mut from = String::new();
    loop {
        let page = s
            .query(&format!("SHOW {object} LIMIT {PAGE}{from}"))
            .await?;
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

fn dataset(name: &'static str, source: &'static str, latency: Option<u32>, rows: usize) -> Dataset {
    Dataset {
        name,
        source,
        latency_minutes: latency,
        rows,
        collected_at: now(),
    }
}

fn usage_error(dataset: &str, e: String) -> String {
    format!("{dataset}: {e} (the active role needs access to SNOWFLAKE.ACCOUNT_USAGE)")
}

/// Splits GRANTS_TO_ROLES rows into role inheritance edges and warehouse privileges.
fn split_role_grants(r: &Rows) -> (Vec<RoleEdge>, Vec<WarehousePrivilege>) {
    let (mut edges, mut privileges) = (Vec::new(), Vec::new());
    for i in 0..r.rows.len() {
        let name = r.get(i, "name").unwrap_or_default();
        let grantee = r.get(i, "grantee_name").unwrap_or_default();
        if r.get(i, "granted_on").as_deref() == Some("ROLE") {
            // GRANT ROLE <name> TO ROLE <grantee>: the grantee (parent) inherits the named (child) role.
            edges.push(RoleEdge {
                parent: grantee,
                child: name,
                granted_by: r.get(i, "granted_by"),
                created_on: r.get(i, "created_on"),
            });
        } else {
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

/// Collects a full metadata snapshot; any failing dataset aborts the whole collection.
pub async fn collect(s: &Session, log: impl Fn(&str)) -> Result<Snapshot, String> {
    let mut snap = Snapshot::default();

    let id = s
        .query("SELECT CURRENT_ACCOUNT() AS ACCOUNT, CURRENT_ROLE() AS ROLE")
        .await?;
    snap.account = id.get(0, "account").unwrap_or_default();
    snap.active_role = id.get(0, "role").unwrap_or_default();

    log("users");
    let r = show_paged(s, "USERS").await?;
    for i in 0..r.rows.len() {
        snap.users.push(User {
            name: r.get(i, "name").unwrap_or_default(),
            login_name: r.get(i, "login_name"),
            default_role: r.get(i, "default_role"),
            disabled: r.get(i, "disabled"),
            created_on: r.get(i, "created_on"),
        });
    }
    snap.datasets
        .push(dataset("users", "SHOW USERS", None, snap.users.len()));

    log("roles");
    let r = show_paged(s, "ROLES").await?;
    for i in 0..r.rows.len() {
        snap.roles.push(Role {
            name: r.get(i, "name").unwrap_or_default(),
            owner: r.get(i, "owner"),
            created_on: r.get(i, "created_on"),
            comment: r.get(i, "comment"),
        });
    }
    snap.datasets
        .push(dataset("roles", "SHOW ROLES", None, snap.roles.len()));

    log("warehouses");
    let r = s.query("SHOW WAREHOUSES").await?;
    for i in 0..r.rows.len() {
        snap.warehouses.push(Warehouse {
            name: r.get(i, "name").unwrap_or_default(),
            owner: r.get(i, "owner"),
            kind: r.get(i, "type"),
            size: r.get(i, "size"),
            created_on: r.get(i, "created_on"),
            comment: r.get(i, "comment"),
        });
    }
    snap.datasets.push(dataset(
        "warehouses",
        "SHOW WAREHOUSES",
        None,
        snap.warehouses.len(),
    ));

    log("user-role grants");
    let r = s
        .query("SELECT TO_VARCHAR(CREATED_ON) AS CREATED_ON, ROLE, GRANTEE_NAME, GRANTED_BY FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS WHERE DELETED_ON IS NULL")
        .await
        .map_err(|e| usage_error("user_roles", e))?;
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
        "ACCOUNT_USAGE.GRANTS_TO_USERS",
        Some(USAGE_LATENCY),
        snap.user_roles.len(),
    ));

    log("role grants");
    let r = s
        .query("SELECT TO_VARCHAR(CREATED_ON) AS CREATED_ON, PRIVILEGE, GRANTED_ON, NAME, GRANTEE_NAME, GRANT_OPTION, GRANTED_BY FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_ROLES WHERE DELETED_ON IS NULL AND GRANTED_TO = 'ROLE' AND ((GRANTED_ON = 'ROLE' AND PRIVILEGE = 'USAGE') OR GRANTED_ON = 'WAREHOUSE')")
        .await
        .map_err(|e| usage_error("role_edges", e))?;
    (snap.role_edges, snap.warehouse_privileges) = split_role_grants(&r);
    let src = "ACCOUNT_USAGE.GRANTS_TO_ROLES";
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
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
