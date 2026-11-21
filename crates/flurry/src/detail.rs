use crate::modes::{Link, Tab};
use crate::store::{Entity, Related, Store, Table};
use flurry_graph::{Access, AccessPath, Graph};

/// One detail pane line; `link` names the entity it opens.
pub struct Line {
    pub text: String,
    pub link: Option<Link>,
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
) -> Result<Vec<Line>, String> {
    let plain = |text: String| Line { text, link: None };
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
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::{Snapshot, Warehouse, WarehousePrivilege};

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
        )
        .unwrap();
        assert_eq!(out[0].text, "owner: SECURITYADMIN");
        assert!(out.iter().any(|l| l.text == "inherits (0):"));
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
        let out = lines(&store, &graph, Entity::Roles, &table, &["ANALYST".into()]).unwrap();
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
        lines(&store, graph, Entity::Users, &table, &["JANE".into()])
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
