//! Shared exact-membership cycle detection (C5-00 consolidation).
//!
//! One 3-color DFS with path-stack cycle extraction, used by every
//! entity-level detector. Two semantics are parameterized because the
//! call sites genuinely differ:
//!
//! - `report_paths` (`true` in specforge-graph's graph.rs): return each
//!   cycle as a closed path `[A, B, A]`;
//! - `cycle_members` (the `cycle_detection` rules): return the set of nodes
//!   that are ON a cycle — nodes that merely lead into one are never
//!   members.
//!
//! It lives here, std-only, so the registry's rules reach it without
//! linking the graph or the parser; `specforge_graph` re-exports it.
//!
//! Determinism (R-6): seeds must be passed sorted, and adjacency is a
//! BTreeMap/BTreeSet pair so traversal order is fixed regardless of HashMap
//! seeding.
//!
//! NOT consolidated here: import-cycle detection (file-level graph, own
//! semantics) and extension peer-dependency cycles (deliberately reports
//! bidirectional pairs A→B→A, which the entity-level pass filters).

use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, Default)]
pub struct CycleOptions {
    /// Emit closed cycle paths (`[A, B, A]`) in addition to the member set.
    pub report_paths: bool,
}

/// Walk `adj` (node -> sorted neighbors) starting from `seeds` (sorted) and
/// return `(sorted cycle member set, optional closed cycle paths)`.
pub fn find_cycles(
    seeds: &[String],
    adj: &BTreeMap<String, BTreeSet<String>>,
    options: CycleOptions,
) -> (BTreeSet<String>, Vec<Vec<String>>) {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: HashMap<&str, Color> =
        seeds.iter().map(|s| (s.as_str(), Color::White)).collect();
    let mut members: BTreeSet<String> = BTreeSet::new();
    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut path: Vec<&str> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adj: &'a BTreeMap<String, BTreeSet<String>>,
        color: &mut HashMap<&'a str, Color>,
        members: &mut BTreeSet<String>,
        paths: &mut Vec<Vec<String>>,
        path: &mut Vec<&'a str>,
        report_paths: bool,
    ) {
        color.insert(node, Color::Gray);
        path.push(node);

        if let Some(neighbors) = adj.get(node) {
            for next in neighbors {
                match color.get(next.as_str()).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        // Back edge: the exact cycle segment is the current
                        // path from `next` onward; feeders into the cycle are
                        // not members.
                        if let Some(pos) = path.iter().position(|&n| n == next.as_str()) {
                            for member in &path[pos..] {
                                members.insert((*member).to_string());
                            }
                            if report_paths {
                                let mut cycle: Vec<String> =
                                    path[pos..].iter().map(|s| (*s).to_string()).collect();
                                cycle.push(next.clone());
                                paths.push(cycle);
                            }
                        }
                    }
                    Color::White => {
                        dfs(
                            next.as_str(),
                            adj,
                            color,
                            members,
                            paths,
                            path,
                            report_paths,
                        );
                    }
                    Color::Black => {}
                }
            }
        }

        path.pop();
        color.insert(node, Color::Black);
    }

    for seed in seeds {
        if color.get(seed.as_str()).copied() == Some(Color::White) {
            dfs(
                seed,
                adj,
                &mut color,
                &mut members,
                &mut paths,
                &mut path,
                options.report_paths,
            );
        }
    }

    (members, paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adj(edges: &[(&str, &str)]) -> BTreeMap<String, BTreeSet<String>> {
        let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (a, b) in edges {
            adj.entry(a.to_string()).or_default().insert(b.to_string());
        }
        adj
    }

    #[test]
    fn simple_cycle_membership_and_path() {
        let a = adj(&[("a", "b"), ("b", "c"), ("c", "a")]);
        let seeds = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (members, paths) = find_cycles(&seeds, &a, CycleOptions { report_paths: true });
        assert_eq!(members.len(), 3);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].first(), paths[0].last());
    }

    #[test]
    fn feeder_into_cycle_is_not_a_member() {
        // feeder -> a -> b -> a: the feeder leads INTO the cycle.
        let a = adj(&[("feeder", "a"), ("a", "b"), ("b", "a")]);
        let seeds = vec!["a".to_string(), "b".to_string(), "feeder".to_string()];
        let (members, _) = find_cycles(&seeds, &a, CycleOptions::default());
        assert!(members.contains("a") && members.contains("b"));
        assert!(!members.contains("feeder"), "feeder must not be a member");
    }

    #[test]
    fn disjoint_cycles_are_independent() {
        let a = adj(&[("a", "b"), ("b", "a"), ("x", "y"), ("y", "x")]);
        let seeds = vec![
            "a".to_string(),
            "b".to_string(),
            "x".to_string(),
            "y".to_string(),
        ];
        let (members, _) = find_cycles(&seeds, &a, CycleOptions::default());
        assert_eq!(members.len(), 4);
    }

    #[test]
    fn deterministic_across_runs() {
        let a = adj(&[("a", "b"), ("b", "c"), ("c", "a"), ("d", "a")]);
        let seeds = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let first = find_cycles(&seeds, &a, CycleOptions { report_paths: true });
        let second = find_cycles(&seeds, &a, CycleOptions { report_paths: true });
        assert_eq!(first, second);
    }
}
