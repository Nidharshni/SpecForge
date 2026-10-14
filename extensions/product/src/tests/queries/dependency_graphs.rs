use super::Rng;
use crate::queries::*;
use crate::tests::G;
use specforge_extension_sdk::prelude::CommandGraph;

#[test]
fn a_diamond_orders_once_and_a_cycle_is_reported_not_followed() {
    // top -> left, right -> base; c1 <-> c2; self -> self; after -> c1.
    let g = G::default()
        .n("top", "feature")
        .n("left", "feature")
        .n("right", "feature")
        .n("base", "feature")
        .n("c1", "feature")
        .n("c2", "feature")
        .n("selfish", "feature")
        .n("after", "feature")
        .edge("top", "left", "depends_on")
        .edge("top", "right", "depends_on")
        .edge("left", "base", "depends_on")
        .edge("right", "base", "depends_on")
        .edge("c1", "c2", "depends_on")
        .edge("c2", "c1", "depends_on")
        .edge("selfish", "selfish", "depends_on")
        .edge("after", "c1", "depends_on")
        .build();
    let order = feature_ordering(&g);
    assert_eq!(
        order.sorted_features,
        ["base", "left", "right", "top", "after", "c1", "c2", "selfish"]
    );
    assert!(order.has_cycles);
    assert_eq!(order.cycle_members, ["c1", "c2", "selfish"]);
}

#[test]
fn a_long_chain_is_walked_without_recursion() {
    let mut g = G::default();
    for i in 0..5000 {
        g = g.n(&format!("m{i:04}"), "module");
        if i > 0 {
            g = g.edge(&format!("m{i:04}"), &format!("m{:04}", i - 1), "depends_on");
        }
    }
    let g = g.edge("m0000", "m4999", "depends_on").build();
    let depth = module_dependency_depth(&g, "m2500").unwrap();
    assert_eq!(depth.depth, -1);
    assert_eq!(depth.longest_chain.len(), 5000);
}

#[test]
fn five_thousand_entities_chain_and_couple_without_recursion() {
    // Each kind a 5000-long chain, plus a hub every module depends on.
    let mut g = G::default().n("hub", "module");
    for kind in ["feature", "milestone", "module"] {
        for i in 0..5000 {
            let id = format!("{kind}{i:04}");
            g = g.n(&id, kind);
            if i > 0 {
                g = g.edge(&id, &format!("{kind}{:04}", i - 1), "depends_on");
            }
            if kind == "module" {
                g = g.edge(&id, "hub", "depends_on");
            }
        }
    }
    let g = g.build();
    let order = feature_ordering(&g);
    assert_eq!(
        order.sorted_features.first().map(String::as_str),
        Some("feature0000")
    );
    assert_eq!(
        order.sorted_features.last().map(String::as_str),
        Some("feature4999")
    );
    assert_eq!(critical_path(&g).path_length, 5000);
    assert_eq!(
        module_dependency_depth(&g, "module4999").unwrap().depth,
        5000
    );
    let coupling = module_coupling(&g);
    assert_eq!(coupling.most_coupled_id.as_deref(), Some("hub"));
    assert_eq!(coupling.modules[0].fan_in, 5000);
}

#[test]
fn a_module_behind_a_cycle_reports_the_cycle_it_reaches() {
    let g = G::default()
        .n("app", "module")
        .n("a", "module")
        .n("b", "module")
        .n("leaf", "module")
        .edge("app", "a", "depends_on")
        .edge("a", "b", "depends_on")
        .edge("b", "a", "depends_on")
        .edge("app", "leaf", "depends_on")
        .build();
    let depth = module_dependency_depth(&g, "app").unwrap();
    assert_eq!(
        (depth.depth, depth.longest_chain),
        (-1, vec!["a".to_string(), "b".to_string()])
    );
    let leaf = module_dependency_depth(&g, "leaf").unwrap();
    assert_eq!(
        (leaf.depth, leaf.longest_chain),
        (0, vec!["leaf".to_string()])
    );
}

/// A random `depends_on` graph over `n` entities of `kind` (`e00`, ...):
/// with `acyclic`, each entity depends only on lower-numbered ones. The
/// entity `i`'s fields are `fields(i)`. Returns the graph and each entity's
/// dependencies.
fn random_graph(
    rng: &mut Rng,
    kind: &str,
    n: usize,
    acyclic: bool,
    fields: impl Fn(&mut Rng) -> Vec<(&'static str, &'static str)>,
) -> (CommandGraph, Vec<Vec<usize>>) {
    let id = |i: usize| format!("e{i:02}");
    let mut g = G::default();
    let mut succ = vec![Vec::new(); n];
    for (i, deps) in succ.iter_mut().enumerate() {
        let f = fields(rng);
        g = g.node(&id(i), kind, &f);
        for j in 0..n {
            if (!acyclic || j < i) && rng.below(4) == 0 {
                g = g.edge(&id(i), &id(j), "depends_on");
                deps.push(j);
            }
        }
    }
    (g.build(), succ)
}

/// Whether `from` reaches `to` over one edge or more.
fn reaches(succ: &[Vec<usize>], from: usize, to: usize) -> bool {
    let mut seen = vec![false; succ.len()];
    let mut stack = succ[from].clone();
    while let Some(u) = stack.pop() {
        if u == to {
            return true;
        }
        if !std::mem::replace(&mut seen[u], true) {
            stack.extend(&succ[u]);
        }
    }
    false
}

/// The longest chain from `u`, in edges, over the entities `keep` admits
/// (acyclic `succ`).
fn longest(succ: &[Vec<usize>], keep: &[bool], u: usize) -> usize {
    succ[u]
        .iter()
        .filter(|&&v| keep[v])
        .map(|&v| 1 + longest(succ, keep, v))
        .max()
        .unwrap_or(0)
}

fn index_of(id: &str) -> usize {
    id[1..].parse().unwrap()
}

#[test]
fn random_dependency_graphs_order_chain_and_report_cycles_as_brute_force_does() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..300 {
        let n = 1 + rng.below(14) as usize;
        let acyclic = round % 2 == 0;
        let on_cycle =
            |succ: &[Vec<usize>]| -> Vec<bool> { (0..n).map(|v| reaches(succ, v, v)).collect() };

        // feature_ordering: every feature once, each after its dependencies
        // unless it is on or behind a cycle; the cycle members exactly.
        let (g, succ) = random_graph(&mut rng, "feature", n, acyclic, |r| match r.below(5) {
            0 => vec![("priority", "critical")],
            1 => vec![("priority", "high")],
            2 => vec![("priority", "low")],
            3 => vec![("priority", "medium")],
            _ => vec![],
        });
        let cyclic = on_cycle(&succ);
        let behind: Vec<bool> = (0..n)
            .map(|v| cyclic[v] || (0..n).any(|c| cyclic[c] && reaches(&succ, v, c)))
            .collect();
        let fo = feature_ordering(&g);
        let at: Vec<usize> = {
            let mut at = vec![usize::MAX; n];
            for (p, id) in fo.sorted_features.iter().enumerate() {
                at[index_of(id)] = p;
            }
            at
        };
        assert_eq!(fo.sorted_features.len(), n, "round {round}");
        assert!(
            at.iter().all(|&p| p < n),
            "round {round}: every feature once"
        );
        for (u, deps) in succ.iter().enumerate() {
            for &v in deps {
                if !behind[u] {
                    assert!(at[v] < at[u], "round {round}: e{v:02} before e{u:02}");
                }
            }
            if behind[u] {
                assert!(
                    (0..n).filter(|&w| !behind[w]).all(|w| at[w] < at[u]),
                    "round {round}: level-less e{u:02} comes last"
                );
            }
        }
        let members: Vec<String> = (0..n)
            .filter(|&v| cyclic[v])
            .map(|v| format!("e{v:02}"))
            .collect();
        assert_eq!(fo.cycle_members, members, "round {round}");
        assert_eq!(fo.has_cycles, !members.is_empty());

        // critical_path: no path while any cycle exists; otherwise a chain
        // of open milestones, earliest first, as long as the longest one.
        let (g, succ) = random_graph(&mut rng, "milestone", n, acyclic, |r| match r.below(3) {
            0 => vec![("status", "completed")],
            1 => vec![("status", "in_progress")],
            _ => vec![],
        });
        let cp = critical_path(&g);
        if on_cycle(&succ).contains(&true) {
            assert!(
                cp.critical_path.is_empty() && cp.message.is_some(),
                "round {round}"
            );
        } else {
            let open: Vec<bool> = (0..n)
                .map(|i| g.node(&format!("e{i:02}")).unwrap().text("status") != Some("completed"))
                .collect();
            let best = (0..n)
                .filter(|&u| open[u])
                .map(|u| 1 + longest(&succ, &open, u))
                .max()
                .unwrap_or(0);
            assert_eq!(cp.path_length, best, "round {round}");
            let path: Vec<usize> = cp
                .critical_path
                .iter()
                .map(|m| index_of(&m.entity_id))
                .collect();
            assert!(
                path.iter().all(|&m| open[m]),
                "round {round}: open milestones only"
            );
            for pair in path.windows(2) {
                assert!(
                    succ[pair[1]].contains(&pair[0]),
                    "round {round}: {path:?} is a chain"
                );
            }
        }

        // module_depth: -1 and the reached cycles' members on or behind a
        // cycle, else the longest chain; module_coupling sums to the edges.
        let (g, succ) = random_graph(&mut rng, "module", n, acyclic, |_| vec![]);
        let cyclic = on_cycle(&succ);
        let all = vec![true; n];
        for u in 0..n {
            let md = module_dependency_depth(&g, &format!("e{u:02}")).unwrap();
            let reached: Vec<String> = (0..n)
                .filter(|&c| cyclic[c] && (c == u || reaches(&succ, u, c)))
                .map(|c| format!("e{c:02}"))
                .collect();
            if reached.is_empty() {
                assert_eq!(md.depth as usize, longest(&succ, &all, u), "round {round}");
                let chain: Vec<usize> = md.longest_chain.iter().map(|m| index_of(m)).collect();
                assert_eq!((chain.len(), chain[0]), (md.depth as usize + 1, u));
                for pair in chain.windows(2) {
                    assert!(succ[pair[0]].contains(&pair[1]), "round {round}: {chain:?}");
                }
            } else {
                assert_eq!((md.depth, md.longest_chain), (-1, reached), "round {round}");
            }
        }
        let mc = module_coupling(&g);
        let edges: usize = succ.iter().map(Vec::len).sum();
        assert_eq!(mc.modules.iter().map(|m| m.fan_in).sum::<usize>(), edges);
        assert_eq!(mc.modules.iter().map(|m| m.fan_out).sum::<usize>(), edges);
    }
}
