use super::{into, of_kind, priority_rank, share, status, targets, text, MEDIUM};
use crate::vocabulary::*;
use serde::Serialize;
use specforge_extension_sdk::prelude::{CommandGraph, GraphNode};

/// The `depends_on` references among one kind's entities, by index into
/// `ids` (sorted, so an index orders as its id does). A reference to an
/// entity of another kind, or one left out, is not followed.
struct DepGraph {
    ids: Vec<String>,
    /// Each entity's dependencies, sorted, once each.
    succ: Vec<Vec<usize>>,
}

impl DepGraph {
    /// The `kind` entities `keep` admits and their `depends_on` among them.
    fn of(graph: &CommandGraph, kind: &str, keep: impl Fn(&GraphNode) -> bool) -> Self {
        let mut ids: Vec<String> = graph
            .nodes_of_kind(kind)
            .filter(|n| keep(n))
            .map(|n| n.id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        let succ = ids
            .iter()
            .map(|id| {
                let mut deps: Vec<usize> = targets(graph, id, "depends_on")
                    .iter()
                    .filter_map(|t| ids.binary_search(t).ok())
                    .collect();
                deps.sort_unstable();
                deps.dedup();
                deps
            })
            .collect();
        DepGraph { ids, succ }
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.ids.binary_search_by(|i| i.as_str().cmp(id)).ok()
    }

    /// Each entity's level (Kahn): 0 for one without dependencies, else one
    /// more than its deepest dependency's, which is the length in edges of
    /// its longest dependency chain. `None` for an entity on a cycle or
    /// depending on one, whose chains do not end.
    fn levels(&self) -> Vec<Option<usize>> {
        let n = self.ids.len();
        let mut dependents = vec![Vec::new(); n];
        for (u, deps) in self.succ.iter().enumerate() {
            for &v in deps {
                dependents[v].push(u);
            }
        }
        let mut remaining: Vec<usize> = self.succ.iter().map(Vec::len).collect();
        let mut level: Vec<Option<usize>> = vec![None; n];
        let mut ready: std::collections::VecDeque<usize> =
            (0..n).filter(|&u| remaining[u] == 0).collect();
        for &u in &ready {
            level[u] = Some(0);
        }
        while let Some(u) = ready.pop_front() {
            let next = level[u].unwrap_or(0) + 1;
            for &d in &dependents[u] {
                level[d] = Some(level[d].map_or(next, |l| l.max(next)));
                remaining[d] -= 1;
                if remaining[d] == 0 {
                    ready.push_back(d);
                }
            }
        }
        // A dependent still waiting on a dependency has no level yet.
        for u in 0..n {
            if remaining[u] > 0 {
                level[u] = None;
            }
        }
        level
    }

    /// The longest dependency chain from `start`, `start` first: at each
    /// step the dependency one level down, the first by id among equals.
    /// `levels` are [`Self::levels`]; `start` must have one.
    fn chain(&self, levels: &[Option<usize>], start: usize) -> Vec<usize> {
        let mut chain = vec![start];
        let mut at = start;
        while let Some(level) = levels[at].filter(|&l| l > 0) {
            let Some(&next) = self.succ[at]
                .iter()
                .find(|&&d| levels[d] == Some(level - 1))
            else {
                break;
            };
            chain.push(next);
            at = next;
        }
        chain
    }

    /// Whether each entity is on a dependency cycle: in a strongly
    /// connected component of two or more, or depending on itself
    /// (Tarjan, iterative, O(V+E)).
    fn in_cycle(&self) -> Vec<bool> {
        const UNSEEN: usize = usize::MAX;
        let n = self.ids.len();
        let (mut index, mut low) = (vec![UNSEEN; n], vec![0; n]);
        let (mut on_stack, mut cyclic) = (vec![false; n], vec![false; n]);
        let (mut stack, mut next) = (Vec::new(), 0);
        for root in 0..n {
            if index[root] != UNSEEN {
                continue;
            }
            index[root] = next;
            low[root] = next;
            next += 1;
            stack.push(root);
            on_stack[root] = true;
            let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
            while let Some(&(v, i)) = calls.last() {
                if let Some(&w) = self.succ[v].get(i) {
                    if let Some(call) = calls.last_mut() {
                        call.1 += 1;
                    }
                    if index[w] == UNSEEN {
                        index[w] = next;
                        low[w] = next;
                        next += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        calls.push((w, 0));
                    } else if on_stack[w] {
                        low[v] = low[v].min(index[w]);
                    }
                    continue;
                }
                calls.pop();
                if let Some(&(parent, _)) = calls.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut component = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        component.push(w);
                        if w == v {
                            break;
                        }
                    }
                    if component.len() > 1 || self.succ[v].contains(&v) {
                        for w in component {
                            cyclic[w] = true;
                        }
                    }
                }
            }
        }
        cyclic
    }

    /// The ids of the entities `cyclic` marks, sorted.
    fn members(&self, cyclic: &[bool]) -> Vec<String> {
        (0..self.ids.len())
            .filter(|&i| cyclic[i])
            .map(|i| self.ids[i].clone())
            .collect()
    }
}

/// `FeatureOrderingPayload`: the features in dependency order.
#[derive(Debug, Serialize)]
pub struct FeatureOrdering {
    pub sorted_features: Vec<String>,
    pub has_cycles: bool,
    pub cycle_members: Vec<String>,
}

/// Every feature once, dependencies before dependents: by level (Kahn),
/// within a level by priority (critical first, none counts as medium),
/// then by id. The features on a cycle or depending on one have no level
/// and come last, ordered the same way; the ones on a cycle are the
/// `cycle_members`.
pub fn feature_ordering(graph: &CommandGraph) -> FeatureOrdering {
    let deps = DepGraph::of(graph, "feature", |_| true);
    let levels = deps.levels();
    let rank = |i: usize| {
        graph
            .node(&deps.ids[i])
            .and_then(|n| n.text("priority"))
            .and_then(priority_rank)
            .unwrap_or(MEDIUM)
    };
    let mut order: Vec<usize> = (0..deps.ids.len()).collect();
    order.sort_by_key(|&i| (levels[i].unwrap_or(usize::MAX), rank(i), i));
    let cycle_members = deps.members(&deps.in_cycle());
    FeatureOrdering {
        sorted_features: order.into_iter().map(|i| deps.ids[i].clone()).collect(),
        has_cycles: !cycle_members.is_empty(),
        cycle_members,
    }
}

/// `CriticalPathNode`.
#[derive(Debug, Serialize)]
pub struct CriticalPathNode {
    pub entity_id: String,
    pub entity_kind: String,
    pub target_date: Option<String>,
    pub status: Option<String>,
    pub slack_days: Option<i64>,
}

/// `CriticalPathPayload`.
#[derive(Debug, Serialize)]
pub struct CriticalPath {
    pub critical_path: Vec<CriticalPathNode>,
    pub path_length: usize,
    pub earliest_completion: Option<String>,
    pub latest_completion: Option<String>,
    pub bottleneck_ids: Vec<String>,
    /// Why there is no path when milestones depend on each other in a
    /// cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The longest `depends_on` chain of milestones not yet `completed`,
/// earliest first (each milestone before the ones depending on it); the
/// first by id among equally long ones. Every node on it has zero slack
/// (none without a `target_date`); the completions are the first's and
/// the last's target dates; the bottlenecks are its `blocked` or
/// `in_progress` milestones. When milestones depend on each other in a
/// cycle (E015) there is no critical path: the path is empty and the
/// message names the cycle.
pub fn critical_path(graph: &CommandGraph) -> CriticalPath {
    let empty = |message: Option<String>| CriticalPath {
        critical_path: Vec::new(),
        path_length: 0,
        earliest_completion: None,
        latest_completion: None,
        bottleneck_ids: Vec::new(),
        message,
    };
    let all = DepGraph::of(graph, "milestone", |_| true);
    let cycle = all.members(&all.in_cycle());
    if !cycle.is_empty() {
        return empty(Some(format!(
            "milestones depend on each other in a cycle ({}): no critical path",
            cycle.join(", ")
        )));
    }
    let open = DepGraph::of(graph, "milestone", |n| {
        status(n) != Some(MILESTONE_COMPLETED)
    });
    let levels = open.levels();
    let Some(start) = (0..open.ids.len()).max_by_key(|&i| (levels[i], std::cmp::Reverse(i))) else {
        return empty(None);
    };
    let mut chain = open.chain(&levels, start);
    chain.reverse();
    let nodes: Vec<CriticalPathNode> = chain
        .iter()
        .filter_map(|&i| graph.node(&open.ids[i]))
        .map(|m| {
            let target_date = text(m, "target_date");
            CriticalPathNode {
                entity_id: m.id.clone(),
                entity_kind: m.kind.clone(),
                slack_days: target_date.as_ref().map(|_| 0),
                target_date,
                status: text(m, "status"),
            }
        })
        .collect();
    CriticalPath {
        path_length: nodes.len(),
        earliest_completion: nodes.first().and_then(|n| n.target_date.clone()),
        latest_completion: nodes.last().and_then(|n| n.target_date.clone()),
        bottleneck_ids: nodes
            .iter()
            .filter(|n| matches!(n.status.as_deref(), Some(s) if s == MILESTONE_BLOCKED || s == MILESTONE_IN_PROGRESS))
            .map(|n| n.entity_id.clone())
            .collect(),
        critical_path: nodes,
        message: None,
    }
}

/// `ModuleDependencyDepthPayload`.
#[derive(Debug, Serialize)]
pub struct ModuleDepth {
    pub module_id: String,
    pub depth: i64,
    pub longest_chain: Vec<String>,
}

/// The longest `depends_on` chain from the module, the module first and
/// the leaf last, and its length in edges. A module on a cycle (E007), or
/// depending on one, has no longest chain: its depth is -1 and the chain
/// is the members of the cycles it reaches, sorted. `None` when
/// `module_id` is not a module.
pub fn module_dependency_depth(graph: &CommandGraph, module_id: &str) -> Option<ModuleDepth> {
    of_kind(graph, module_id, "module")?;
    let deps = DepGraph::of(graph, "module", |_| true);
    let start = deps.index(module_id)?;
    let levels = deps.levels();
    let (depth, longest_chain) = match levels[start] {
        Some(depth) => (
            depth as i64,
            deps.chain(&levels, start)
                .into_iter()
                .map(|i| deps.ids[i].clone())
                .collect(),
        ),
        None => {
            let cyclic = deps.in_cycle();
            let mut reached = vec![false; deps.ids.len()];
            let mut frontier = vec![start];
            reached[start] = true;
            while let Some(u) = frontier.pop() {
                for &v in &deps.succ[u] {
                    if !reached[v] {
                        reached[v] = true;
                        frontier.push(v);
                    }
                }
            }
            let on_reached_cycle: Vec<bool> = (0..deps.ids.len())
                .map(|i| reached[i] && cyclic[i])
                .collect();
            (-1, deps.members(&on_reached_cycle))
        }
    };
    Some(ModuleDepth {
        module_id: module_id.to_string(),
        depth,
        longest_chain,
    })
}

/// `ModuleCouplingEntry`.
#[derive(Debug, Serialize)]
pub struct ModuleCouplingEntry {
    pub module_id: String,
    pub fan_in: usize,
    pub fan_out: usize,
    pub coupling: usize,
}

/// `ModuleCouplingPayload` before paging.
#[derive(Debug)]
pub struct ModuleCoupling {
    pub modules: Vec<ModuleCouplingEntry>,
    pub avg_fan_in: Option<f64>,
    pub avg_fan_out: Option<f64>,
    pub most_coupled_id: Option<String>,
    pub total_modules: usize,
}

/// Every module's fan-in (the modules depending on it) and fan-out (the
/// modules it depends on), counting each other module once, most coupled
/// first, ties by id; the averages are over every module (`null` without
/// modules).
pub fn module_coupling(graph: &CommandGraph) -> ModuleCoupling {
    let deps = DepGraph::of(graph, "module", |_| true);
    let mut fan_in = vec![0; deps.ids.len()];
    for d in deps.succ.iter().flatten() {
        fan_in[*d] += 1;
    }
    let mut modules: Vec<ModuleCouplingEntry> = deps
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| ModuleCouplingEntry {
            module_id: id.clone(),
            fan_in: fan_in[i],
            fan_out: deps.succ[i].len(),
            coupling: fan_in[i] + deps.succ[i].len(),
        })
        .collect();
    modules.sort_by(|a, b| {
        b.coupling
            .cmp(&a.coupling)
            .then_with(|| a.module_id.cmp(&b.module_id))
    });
    let total = modules.len();
    let edges: usize = deps.succ.iter().map(Vec::len).sum();
    ModuleCoupling {
        avg_fan_in: share(edges, total),
        avg_fan_out: share(edges, total),
        most_coupled_id: modules.first().map(|m| m.module_id.clone()),
        total_modules: total,
        modules,
    }
}

/// `DeliverableDependentPayload`: the deliverables that declare
/// `depends_on` the deliverable, sorted by id.
#[derive(Debug, Serialize)]
pub struct DeliverableDependents {
    pub deliverable_id: String,
    pub dependents: Vec<String>,
    pub count: usize,
}

/// The deliverables that declare `depends_on` the deliverable; `None`
/// when `deliverable_id` is not a deliverable.
pub fn deliverable_dependents(
    graph: &CommandGraph,
    deliverable_id: &str,
) -> Option<DeliverableDependents> {
    of_kind(graph, deliverable_id, "deliverable")?;
    let dependents = into(graph, deliverable_id, "depends_on", "deliverable");
    Some(DeliverableDependents {
        deliverable_id: deliverable_id.to_string(),
        count: dependents.len(),
        dependents,
    })
}
