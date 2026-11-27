use crate::modes::{Link, Tab};
use crate::store::{Entity, Related, Store, Table};
use flurry_graph::{Access, AccessPath, Graph};

/// Fold key of the lineage tree in `State::expanded`; grant types are upper-case so it cannot clash.
pub const LINEAGE: &str = "lineage";

/// One detail pane line; `link` names the entity it opens and `fold` the grant type or tree it unfolds.
pub struct Line {
    pub text: String,
    pub link: Option<Link>,
    pub fold: Option<String>,
}

fn plain(text: String) -> Line {
    Line {
        text,
        link: None,
        fold: None,
    }
}

// Grants of a role grouped by object type, one fold line per type; `expanded` types also list their grants.
fn grant_lines(store: &Store, role: &str, expanded: &[String]) -> Result<Vec<Line>, String> {
    let Some(rows) = store.grants(role)? else {
        return Ok(vec![plain("grants: not collected, press R to sync".into())]);
    };
    let mut out = vec![plain(format!("grants ({}):", rows.len()))];
    for group in rows.chunk_by(|a, b| a[0] == b[0]) {
        let kind = &group[0][0];
        let open = expanded.contains(kind);
        out.push(Line {
            text: format!("{} {kind} ({})", if open { "▾" } else { "▸" }, group.len()),
            link: None,
            fold: Some(kind.clone()),
        });
        if !open {
            continue;
        }
        let tab = match kind.as_str() {
            "ROLE" => Some(Tab::Roles),
            "USER" => Some(Tab::Users),
            "WAREHOUSE" => Some(Tab::Warehouses),
            _ => None,
        };
        out.extend(group.iter().map(|r| Line {
            text: format!("    {} on {}", r[1], r[2]),
            link: tab.map(|tab| Link {
                tab,
                name: r[2].clone(),
            }),
            fold: None,
        }));
    }
    Ok(out)
}

// Roles inheriting `role`, as a tree under a fold line; the tree is only listed when `expanded`.
fn lineage_lines(graph: &Graph, role: &str, expanded: &[String]) -> Vec<Line> {
    let tree = graph.lineage(role);
    let open = expanded.iter().any(|f| f == LINEAGE);
    let roots = tree.iter().filter(|n| !n.repeated).count();
    let mut out = vec![Line {
        text: format!("{} lineage ({roots})", if open { "▾" } else { "▸" }),
        link: None,
        fold: Some(LINEAGE.into()),
    }];
    if open {
        out.extend(tree.into_iter().map(|n| Line {
            text: format!(
                "{}{}{}",
                "  ".repeat(n.depth),
                n.role,
                if n.repeated { " (see above)" } else { "" }
            ),
            link: Some(Link {
                tab: Tab::Roles,
                name: n.role,
            }),
            fold: None,
        }));
    }
    out
}

pub fn chain(path: &AccessPath) -> String {
    let roles = path.roles.join(" -> ");
    if path.implicit {
        format!("{roles} (implicit)")
    } else {
        roles
    }
}

fn access_verdict(graph: &Graph, user: &str, warehouse: &str) -> String {
    match graph.access(user, warehouse, "USAGE") {
        Access::Granted { paths, truncated } => {
            let extra = paths.len() - 1;
            let plus = if truncated { "+" } else { "" };
            let more = if extra > 0 || truncated {
                format!(" (+{extra}{plus} more)")
            } else {
                String::new()
            };
            format!("ACCESS via {}{more}", chain(&paths[0]))
        }
        Access::NoKnownAccess => "NO ACCESS".into(),
        Access::Unknown { .. } => "UNKNOWN / INCOMPLETE".into(),
    }
}

fn tab_of(entity: Entity) -> Tab {
    match entity {
        Entity::Users => Tab::Users,
        Entity::Roles => Tab::Roles,
        Entity::Warehouses => Tab::Warehouses,
    }
}

// Detail lines of one list row: its columns, then the related entities; roles add their inheritance chains.
pub fn lines(
    store: &Store,
    graph: &Graph,
    entity: Entity,
    table: &Table,
    row: &[String],
    expanded: &[String],
) -> Result<Vec<Line>, String> {
    let mut out: Vec<Line> = table
        .headers
        .iter()
        .zip(row)
        .skip(1)
        .map(|(h, v)| plain(format!("{h}: {}", if v.is_empty() { "-" } else { v })))
        .collect();
    let mut sections = store.related(entity, &row[0])?;
    if matches!(entity, Entity::Roles) {
        let roles = |label, names: Vec<String>| Related {
            label,
            target: Entity::Roles,
            items: names.into_iter().map(|n| (n.clone(), n)).collect(),
        };
        sections.push(roles("inherits", graph.inherited_roles(&row[0])));
        sections.push(roles("inherited by", graph.inheriting_roles(&row[0])));
    }
    if matches!(entity, Entity::Users) {
        let warehouses = store.list(Entity::Warehouses, "")?.rows;
        sections.push(Related {
            label: "warehouse access (USAGE)",
            target: Entity::Warehouses,
            items: warehouses
                .into_iter()
                .map(|w| {
                    (
                        format!("{}  {}", w[0], access_verdict(graph, &row[0], &w[0])),
                        w[0].clone(),
                    )
                })
                .collect(),
        });
    }
    for r in sections {
        let tab = tab_of(r.target);
        out.push(plain(String::new()));
        out.push(plain(format!("{} ({}):", r.label, r.items.len())));
        out.extend(r.items.into_iter().map(|(text, name)| Line {
            text: format!("  {text}"),
            link: Some(Link { tab, name }),
            fold: None,
        }));
    }
    if matches!(entity, Entity::Roles) {
        out.push(plain(String::new()));
        out.extend(lineage_lines(graph, &row[0], expanded));
        out.push(plain(String::new()));
        out.extend(grant_lines(store, &row[0], expanded)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::{Dataset, Grant, Snapshot, Warehouse, WarehousePrivilege};

    #[test]
    fn role_detail_lists_fields_and_sections() {
        let store = Store::in_memory();
        let table = Table {
            headers: vec!["name".into(), "owner".into()],
            rows: vec![],
        };
        let out = lines(
            &store,
            &Graph::default(),
            Entity::Roles,
            &table,
            &["ANALYST".into(), "SECURITYADMIN".into()],
            &[],
        )
        .unwrap();
        assert_eq!(out[0].text, "owner: SECURITYADMIN");
        assert!(out.iter().any(|l| l.text == "inherits (0):"));
    }

    #[test]
    fn lineage_is_folded_until_expanded() {
        let mut graph = Graph::default();
        graph.add_role_edge("LEAD", "ANALYST");
        graph.add_role_edge("ADMIN", "LEAD");
        let store = Store::in_memory();
        let table = Table {
            headers: vec!["name".into()],
            rows: vec![],
        };
        let text = |expanded: &[String]| -> Vec<String> {
            lines(
                &store,
                &graph,
                Entity::Roles,
                &table,
                &["ANALYST".into()],
                expanded,
            )
            .unwrap()
            .into_iter()
            .map(|l| l.text)
            .collect()
        };
        let folded = text(&[]);
        assert!(folded.contains(&"▸ lineage (2)".to_string()));
        assert!(!folded.contains(&"    ADMIN".to_string()));
        let open = text(&[LINEAGE.to_string()]);
        let at = open.iter().position(|l| l == "▾ lineage (2)").unwrap();
        assert_eq!(open[at + 1..at + 3], ["  LEAD", "    ADMIN"]);
    }

    #[test]
    fn related_lines_link_to_their_entity() {
        let mut store = Store::in_memory();
        let snapshot = Snapshot {
            warehouse_privileges: vec![WarehousePrivilege {
                role: "ANALYST".into(),
                warehouse: "WH_A".into(),
                privilege: "USAGE".into(),
                grant_option: None,
                granted_by: None,
            }],
            ..Snapshot::default()
        };
        store.save("test", &snapshot).unwrap();
        let mut graph = Graph::default();
        graph.add_role_edge("ANALYST", "READER");
        let table = Table {
            headers: vec!["name".into()],
            rows: vec![],
        };
        let out = lines(
            &store,
            &graph,
            Entity::Roles,
            &table,
            &["ANALYST".into()],
            &[],
        )
        .unwrap();
        let link = |text: &str| {
            out.iter()
                .find(|l| l.text == text)
                .and_then(|l| l.link.clone())
        };
        assert_eq!(
            link("  USAGE on WH_A"),
            Some(Link {
                tab: Tab::Warehouses,
                name: "WH_A".into()
            })
        );
        assert_eq!(
            link("  READER"),
            Some(Link {
                tab: Tab::Roles,
                name: "READER".into()
            })
        );
        assert_eq!(link("inherits (1):"), None);
    }

    #[test]
    fn role_grants_fold_by_type_and_unfold_on_demand() {
        let mut store = Store::in_memory();
        let grant = |privilege: &str, granted_on: &str, name: &str| Grant {
            role: "ANALYST".into(),
            privilege: privilege.into(),
            granted_on: granted_on.into(),
            name: name.into(),
            grant_option: None,
            granted_by: None,
        };
        let snapshot = Snapshot {
            grants: vec![
                grant("SELECT", "TABLE", "DB.S.T2"),
                grant("USAGE", "DATABASE", "DB"),
                grant("SELECT", "TABLE", "DB.S.T1"),
                grant("USAGE", "WAREHOUSE", "WH_A"),
            ],
            datasets: vec![Dataset {
                name: "role_grants".into(),
                source: "test",
                latency_minutes: None,
                rows: 4,
                collected_at: 0,
                error: None,
            }],
            ..Snapshot::default()
        };
        store.save("test", &snapshot).unwrap();
        let texts = |expanded: &[String]| -> Vec<String> {
            let out = grant_lines(&store, "ANALYST", expanded).unwrap();
            out.into_iter().map(|l| l.text).collect()
        };
        assert_eq!(
            texts(&[]),
            [
                "grants (4):",
                "▸ DATABASE (1)",
                "▸ TABLE (2)",
                "▸ WAREHOUSE (1)"
            ]
        );
        assert_eq!(
            texts(&["TABLE".into(), "WAREHOUSE".into()]),
            [
                "grants (4):",
                "▸ DATABASE (1)",
                "▾ TABLE (2)",
                "    SELECT on DB.S.T1",
                "    SELECT on DB.S.T2",
                "▾ WAREHOUSE (1)",
                "    USAGE on WH_A",
            ]
        );
        let out = grant_lines(&store, "ANALYST", &["WAREHOUSE".into()]).unwrap();
        assert_eq!(out[1].fold.as_deref(), Some("DATABASE"));
        assert_eq!(out.last().unwrap().link.as_ref().unwrap().name, "WH_A");
    }

    #[test]
    fn grants_of_an_older_snapshot_are_not_collected() {
        let store = Store::in_memory();
        let out = grant_lines(&store, "ANALYST", &[]).unwrap();
        assert_eq!(out[0].text, "grants: not collected, press R to sync");
    }

    fn warehouse(name: &str) -> Warehouse {
        Warehouse {
            name: name.into(),
            owner: None,
            kind: None,
            size: None,
            created_on: None,
            comment: None,
        }
    }

    fn access_lines(graph: &Graph) -> Vec<String> {
        let mut store = Store::in_memory();
        let snapshot = Snapshot {
            warehouses: vec![warehouse("WH_A"), warehouse("WH_B")],
            ..Snapshot::default()
        };
        store.save("test", &snapshot).unwrap();
        let table = Table {
            headers: vec!["name".into()],
            rows: vec![],
        };
        lines(&store, graph, Entity::Users, &table, &["JANE".into()], &[])
            .unwrap()
            .into_iter()
            .map(|l| l.text)
            .collect()
    }

    #[test]
    fn user_detail_explains_warehouse_access() {
        let mut graph = Graph::default();
        graph.add_user_role("JANE", "ANALYST");
        graph.add_role_edge("ANALYST", "READER");
        graph.add_warehouse_privilege("READER", "WH_A", "USAGE");
        let out = access_lines(&graph);
        assert!(out.contains(&"warehouse access (USAGE) (2):".to_string()));
        assert!(out.contains(&"  WH_A  ACCESS via ANALYST -> READER".to_string()));
        assert!(out.contains(&"  WH_B  NO ACCESS".to_string()));
    }

    #[test]
    fn incomplete_collection_is_unknown_not_no_access() {
        let mut graph = Graph::default();
        graph.mark_incomplete("warehouse_privileges");
        let out = access_lines(&graph);
        assert!(out.contains(&"  WH_B  UNKNOWN / INCOMPLETE".to_string()));
    }
}
