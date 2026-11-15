//! Pure in-memory RBAC graph: role traversal and user-to-warehouse access explanations.

use std::collections::{BTreeMap, BTreeSet};

/// Every user implicitly holds this role.
pub const PUBLIC: &str = "PUBLIC";

/// Datasets whose completeness decides whether "no access" can be claimed.
const ACCESS_DATASETS: [&str; 3] = ["user_roles", "role_edges", "warehouse_privileges"];

const MAX_PATHS: usize = 100;

type Names = BTreeMap<String, BTreeSet<String>>;

#[derive(Default)]
pub struct Graph {
    user_roles: Names,
    /// parent role -> roles it inherits
    children: Names,
    /// child role -> roles that inherit it
    parents: Names,
    /// (role, warehouse) -> privileges, upper-cased
    privileges: BTreeMap<(String, String), BTreeSet<String>>,
    incomplete: BTreeSet<String>,
}

/// One way a user reaches a privilege: the role chain from an assigned role down to the granting role.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccessPath {
    /// True when the first role is the implicit `PUBLIC`, not an assigned one.
    pub implicit: bool,
    pub roles: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Access {
    Granted {
        paths: Vec<AccessPath>,
        /// True when more paths exist than were listed.
        truncated: bool,
    },
    /// Collection was complete and no path exists.
    NoKnownAccess,
    /// No path found, but these datasets were not collected, so access cannot be ruled out.
    Unknown { missing: Vec<String> },
}

impl Graph {
    pub fn add_user_role(&mut self, user: &str, role: &str) {
        self.user_roles
            .entry(user.into())
            .or_default()
            .insert(role.into());
    }

    /// `parent` inherits `child` (`GRANT ROLE child TO ROLE parent`).
    pub fn add_role_edge(&mut self, parent: &str, child: &str) {
        self.children
            .entry(parent.into())
            .or_default()
            .insert(child.into());
        self.parents
            .entry(child.into())
            .or_default()
            .insert(parent.into());
    }

    pub fn add_warehouse_privilege(&mut self, role: &str, warehouse: &str, privilege: &str) {
        self.privileges
            .entry((role.into(), warehouse.into()))
            .or_default()
            .insert(privilege.to_uppercase());
    }

    /// Flags a dataset as failed or missing in the snapshot.
    pub fn mark_incomplete(&mut self, dataset: &str) {
        self.incomplete.insert(dataset.into());
    }

    /// False when the role inheritance dataset failed, so traversals may miss roles.
    pub fn role_edges_complete(&self) -> bool {
        !self.incomplete.contains("role_edges")
    }

    /// Roles `role` inherits, directly or transitively.
    pub fn inherited_roles(&self, role: &str) -> Vec<String> {
        reach(&self.children, role)
    }

    /// Roles that inherit `role`, directly or transitively.
    pub fn inheriting_roles(&self, role: &str) -> Vec<String> {
        reach(&self.parents, role)
    }

    /// Why `user` can or cannot hold `privilege` on `warehouse`.
    pub fn access(&self, user: &str, warehouse: &str, privilege: &str) -> Access {
        let privilege = privilege.to_uppercase();
        let holders: BTreeSet<&str> = self
            .privileges
            .iter()
            .filter(|((_, wh), p)| wh == warehouse && p.contains(&privilege))
            .map(|((role, _), _)| role.as_str())
            .collect();
        // Roles that reach a holder; pruning to them keeps a miss linear instead of exploring every path.
        let mut useful: BTreeSet<String> = holders.iter().map(|r| r.to_string()).collect();
        for h in &holders {
            useful.extend(reach(&self.parents, h));
        }
        let assigned = self.user_roles.get(user).cloned().unwrap_or_default();
        let mut starts: Vec<(bool, &str)> = assigned.iter().map(|r| (false, r.as_str())).collect();
        if !assigned.contains(PUBLIC) {
            starts.push((true, PUBLIC));
        }
        let mut walk = Walk {
            graph: self,
            holders: &holders,
            useful: &useful,
            implicit: false,
            chain: Vec::new(),
            found: Vec::new(),
        };
        for (implicit, start) in starts {
            if useful.contains(start) {
                walk.implicit = implicit;
                walk.chain = vec![start];
                walk.visit(start);
            }
        }
        let mut paths = walk.found;
        let truncated = paths.len() > MAX_PATHS;
        paths.truncate(MAX_PATHS);
        paths.sort();
        if !paths.is_empty() {
            return Access::Granted { paths, truncated };
        }
        let missing: Vec<String> = ACCESS_DATASETS
            .iter()
            .filter(|d| self.incomplete.contains(**d))
            .map(|d| d.to_string())
            .collect();
        if missing.is_empty() {
            Access::NoKnownAccess
        } else {
            Access::Unknown { missing }
        }
    }
}

// Breadth-first reachability that ignores cycles and never returns `start`.
fn reach(edges: &Names, start: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![start.to_string()];
    while let Some(role) = queue.pop() {
        for next in edges.get(&role).into_iter().flatten() {
            if next != start && seen.insert(next.clone()) {
                queue.push(next.clone());
            }
        }
    }
    seen.into_iter().collect()
}

struct Walk<'a> {
    graph: &'a Graph,
    holders: &'a BTreeSet<&'a str>,
    useful: &'a BTreeSet<String>,
    implicit: bool,
    chain: Vec<&'a str>,
    found: Vec<AccessPath>,
}

impl<'a> Walk<'a> {
    // Depth-first over inherited roles that reach a holder; a branch stops at the first holder, the walk after MAX_PATHS + 1 paths.
    fn visit(&mut self, role: &'a str) {
        if self.found.len() > MAX_PATHS {
            return;
        }
        if self.holders.contains(role) {
            self.found.push(AccessPath {
                implicit: self.implicit,
                roles: self.chain.iter().map(|r| r.to_string()).collect(),
            });
            return;
        }
        for child in self.graph.children.get(role).into_iter().flatten() {
            if self.useful.contains(child) && !self.chain.contains(&child.as_str()) {
                self.chain.push(child);
                self.visit(child);
                self.chain.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(a: Access) -> Vec<Vec<String>> {
        match a {
            Access::Granted { paths, .. } => paths.into_iter().map(|p| p.roles).collect(),
            other => panic!("expected access, got {other:?}"),
        }
    }

    fn chain(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn role_edges_completeness_follows_the_failed_dataset() {
        let mut g = Graph::default();
        assert!(g.role_edges_complete());
        g.mark_incomplete("user_roles");
        assert!(g.role_edges_complete());
        g.mark_incomplete("role_edges");
        assert!(!g.role_edges_complete());
    }

    #[test]
    fn direct_grant_through_an_assigned_role() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "ANALYST");
        g.add_warehouse_privilege("ANALYST", "WH", "USAGE");
        assert_eq!(
            paths(g.access("ALICE", "WH", "usage")),
            [chain(&["ANALYST"])]
        );
    }

    #[test]
    fn parent_role_inherits_child_privilege() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "LEAD");
        g.add_role_edge("LEAD", "ANALYST");
        g.add_warehouse_privilege("ANALYST", "WH", "USAGE");
        assert_eq!(
            paths(g.access("ALICE", "WH", "USAGE")),
            [chain(&["LEAD", "ANALYST"])]
        );
        assert_eq!(g.inherited_roles("LEAD"), ["ANALYST"]);
        assert_eq!(g.inheriting_roles("ANALYST"), ["LEAD"]);
        // Inheritance is one-way: the child does not gain the parent's grants.
        g.add_user_role("BOB", "ANALYST");
        g.add_warehouse_privilege("LEAD", "WH2", "USAGE");
        assert_eq!(g.access("BOB", "WH2", "USAGE"), Access::NoKnownAccess);
    }

    #[test]
    fn multiple_paths_are_all_listed_without_repeating_roles() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "TOP");
        g.add_role_edge("TOP", "LEFT");
        g.add_role_edge("TOP", "RIGHT");
        g.add_role_edge("LEFT", "BASE");
        g.add_role_edge("RIGHT", "BASE");
        g.add_warehouse_privilege("BASE", "WH", "USAGE");
        let p = paths(g.access("ALICE", "WH", "USAGE"));
        assert_eq!(
            p,
            [
                chain(&["TOP", "LEFT", "BASE"]),
                chain(&["TOP", "RIGHT", "BASE"])
            ]
        );
    }

    #[test]
    fn cycles_terminate() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "A");
        g.add_role_edge("A", "B");
        g.add_role_edge("B", "A");
        assert_eq!(g.access("ALICE", "WH", "USAGE"), Access::NoKnownAccess);
        assert_eq!(g.inherited_roles("A"), ["B"]);
        assert_eq!(g.inheriting_roles("A"), ["B"]);
    }

    #[test]
    fn public_is_an_implicit_role() {
        let mut g = Graph::default();
        g.add_warehouse_privilege(PUBLIC, "WH", "USAGE");
        let Access::Granted { paths, .. } = g.access("NEW_USER", "WH", "USAGE") else {
            panic!("expected access");
        };
        assert_eq!(
            paths,
            [AccessPath {
                implicit: true,
                roles: chain(&[PUBLIC])
            }]
        );
    }

    #[test]
    fn privilege_kinds_are_distinct() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "OPS");
        g.add_warehouse_privilege("OPS", "WH", "MONITOR");
        assert_eq!(g.access("ALICE", "WH", "USAGE"), Access::NoKnownAccess);
    }

    #[test]
    fn complete_data_without_a_path_is_no_known_access() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "ANALYST");
        assert_eq!(g.access("ALICE", "WH", "USAGE"), Access::NoKnownAccess);
    }

    #[test]
    fn failed_dataset_makes_a_missing_path_unknown() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "ANALYST");
        g.mark_incomplete("warehouse_privileges");
        g.mark_incomplete("role_edges");
        g.mark_incomplete("users");
        assert_eq!(
            g.access("ALICE", "WH", "USAGE"),
            Access::Unknown {
                missing: chain(&["role_edges", "warehouse_privileges"])
            }
        );
    }

    #[test]
    fn a_found_path_stays_definitive_when_data_is_incomplete() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "ANALYST");
        g.add_warehouse_privilege("ANALYST", "WH", "USAGE");
        g.mark_incomplete("role_edges");
        assert!(matches!(
            g.access("ALICE", "WH", "USAGE"),
            Access::Granted { .. }
        ));
    }

    // 2^24 simple paths: an exhaustive walk would not finish.
    fn layered(g: &mut Graph) {
        g.add_user_role("ALICE", "L0_0");
        for l in 0..24 {
            for a in 0..2 {
                for b in 0..2 {
                    g.add_role_edge(&format!("L{l}_{a}"), &format!("L{}_{b}", l + 1));
                }
            }
        }
    }

    #[test]
    fn dense_hierarchy_without_access_is_fast() {
        let mut g = Graph::default();
        layered(&mut g);
        assert_eq!(g.access("ALICE", "WH", "USAGE"), Access::NoKnownAccess);
    }

    #[test]
    fn dense_hierarchy_with_access_stops_at_the_cap() {
        let mut g = Graph::default();
        layered(&mut g);
        g.add_warehouse_privilege("L24_0", "WH", "USAGE");
        let Access::Granted { paths, truncated } = g.access("ALICE", "WH", "USAGE") else {
            panic!("expected access");
        };
        assert_eq!(paths.len(), MAX_PATHS);
        assert!(truncated);
    }

    #[test]
    fn path_listing_is_capped() {
        let mut g = Graph::default();
        g.add_user_role("ALICE", "TOP");
        for i in 0..(MAX_PATHS + 20) {
            let mid = format!("MID_{i:03}");
            g.add_role_edge("TOP", &mid);
            g.add_warehouse_privilege(&mid, "WH", "USAGE");
        }
        let Access::Granted { paths, truncated } = g.access("ALICE", "WH", "USAGE") else {
            panic!("expected access");
        };
        assert_eq!(paths.len(), MAX_PATHS);
        assert!(truncated);
    }
}
