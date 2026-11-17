use crate::store::{Entity, Store, Table};
use flurry_graph::Graph;

// Detail lines of one list row: its columns, then the related names; roles add their inheritance chains.
pub fn lines(
    store: &Store,
    graph: &Graph,
    entity: Entity,
    table: &Table,
    row: &[String],
) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = table
        .headers
        .iter()
        .zip(row)
        .skip(1)
        .map(|(h, v)| format!("{h}: {}", if v.is_empty() { "-" } else { v }))
        .collect();
    let mut sections = store.related(entity, &row[0])?;
    if matches!(entity, Entity::Roles) {
        sections.push(("inherits", graph.inherited_roles(&row[0])));
        sections.push(("inherited by", graph.inheriting_roles(&row[0])));
    }
    for (label, values) in sections {
        out.push(String::new());
        out.push(format!("{label} ({}):", values.len()));
        out.extend(values.into_iter().map(|v| format!("  {v}")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(out[0], "owner: SECURITYADMIN");
        assert!(out.contains(&"inherits (0):".to_string()));
    }
}
