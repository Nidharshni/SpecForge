use super::{of_kind, share, sorted_dedup, targets};
use serde::Serialize;
use specforge_extension_sdk::prelude::CommandGraph;

/// The most `see_also` hops `term_graph` follows; more are clamped to it.
pub const MAX_TERM_HOPS: usize = 5;

/// The `see_also` references between terms: every term by id, and each
/// one's targets (`out`) and its neighbours either way (`adjacent`), by
/// index, sorted, once each. A term's reference to itself, or to an
/// entity that is not a term, is not one.
struct TermGraph {
    ids: Vec<String>,
    out: Vec<Vec<usize>>,
    adjacent: Vec<Vec<usize>>,
}

impl TermGraph {
    fn of(graph: &CommandGraph) -> Self {
        let ids = sorted_dedup(graph.nodes_of_kind("term").map(|t| t.id.clone()).collect());
        let mut out = vec![Vec::new(); ids.len()];
        let mut adjacent = vec![Vec::new(); ids.len()];
        for (u, id) in ids.iter().enumerate() {
            for target in targets(graph, id, "see_also") {
                match ids.binary_search(&target) {
                    Ok(v) if v != u => {
                        out[u].push(v);
                        adjacent[u].push(v);
                        adjacent[v].push(u);
                    }
                    _ => {}
                }
            }
        }
        for list in out.iter_mut().chain(adjacent.iter_mut()) {
            list.sort_unstable();
            list.dedup();
        }
        TermGraph { ids, out, adjacent }
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.ids.binary_search_by(|i| i.as_str().cmp(id)).ok()
    }
}

/// `TermGraphPayload`.
#[derive(Debug, Serialize)]
pub struct TermGraphPayload {
    pub term_id: String,
    pub related_terms: Vec<String>,
    pub max_hops: usize,
}

/// The terms the term reaches over its `see_also` references within
/// `max_hops` (default 1, clamped to [`MAX_TERM_HOPS`]; 0 reaches none),
/// breadth first, each once, sorted by id, the term itself left out.
/// `None` when `term_id` is not a term.
pub fn term_graph(
    graph: &CommandGraph,
    term_id: &str,
    max_hops: Option<usize>,
) -> Option<TermGraphPayload> {
    of_kind(graph, term_id, "term")?;
    let terms = TermGraph::of(graph);
    let start = terms.index(term_id)?;
    let max_hops = max_hops.unwrap_or(1).min(MAX_TERM_HOPS);
    let mut seen = vec![false; terms.ids.len()];
    seen[start] = true;
    let mut frontier = vec![start];
    for _ in 0..max_hops {
        let mut next = Vec::new();
        for u in frontier {
            for &v in &terms.out[u] {
                if !std::mem::replace(&mut seen[v], true) {
                    next.push(v);
                }
            }
        }
        frontier = next;
    }
    seen[start] = false;
    Some(TermGraphPayload {
        term_id: term_id.to_string(),
        related_terms: (0..terms.ids.len())
            .filter(|&i| seen[i])
            .map(|i| terms.ids[i].clone())
            .collect(),
        max_hops,
    })
}

/// `TermCluster`.
#[derive(Debug, Serialize)]
pub struct TermCluster {
    pub cluster_id: usize,
    pub term_ids: Vec<String>,
    pub term_count: usize,
}

/// `TermClusterPayload`.
#[derive(Debug, Serialize)]
pub struct TermClusters {
    pub clusters: Vec<TermCluster>,
    pub cluster_count: usize,
    pub isolated_count: usize,
    pub total_terms: usize,
}

/// The connected components of the `see_also` graph between terms, a
/// reference read either way: each a cluster of its terms sorted by id,
/// largest first, ties by first term id, numbered from 1 in that order.
/// A term without a `see_also` link either way is isolated, in no
/// cluster.
pub fn term_clusters(graph: &CommandGraph) -> TermClusters {
    let terms = TermGraph::of(graph);
    let n = terms.ids.len();
    let mut seen = vec![false; n];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    let mut isolated_count = 0;
    for root in 0..n {
        if seen[root] {
            continue;
        }
        if terms.adjacent[root].is_empty() {
            isolated_count += 1;
            continue;
        }
        seen[root] = true;
        let (mut component, mut stack) = (vec![root], vec![root]);
        while let Some(u) = stack.pop() {
            for &v in &terms.adjacent[u] {
                if !std::mem::replace(&mut seen[v], true) {
                    component.push(v);
                    stack.push(v);
                }
            }
        }
        component.sort_unstable();
        clusters.push(component);
    }
    // Ids are sorted, so a component's first index is its first id.
    clusters.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    let clusters: Vec<TermCluster> = clusters
        .into_iter()
        .enumerate()
        .map(|(i, c)| TermCluster {
            cluster_id: i + 1,
            term_count: c.len(),
            term_ids: c.into_iter().map(|t| terms.ids[t].clone()).collect(),
        })
        .collect();
    TermClusters {
        cluster_count: clusters.len(),
        clusters,
        isolated_count,
        total_terms: n,
    }
}

/// `TermDensityPayload`.
#[derive(Debug, Serialize)]
pub struct TermDensity {
    pub total_terms: usize,
    pub total_see_also: usize,
    pub avg_connections: Option<f64>,
    pub max_connections: usize,
    pub hub_terms: Vec<String>,
    pub isolated_terms: Vec<String>,
}

/// How connected the glossary is: the terms, the `see_also` references
/// between them (each source and target pair once), their average per
/// term (`null` without terms) and each term's connections (the terms it
/// links to or is linked from, each once). A hub has more than twice the
/// average connections and at least 3; an isolated term has none. Both
/// sorted by id.
pub fn term_density(graph: &CommandGraph) -> TermDensity {
    let terms = TermGraph::of(graph);
    let n = terms.ids.len();
    let total_see_also: usize = terms.out.iter().map(Vec::len).sum();
    let avg_connections = share(total_see_also, n);
    let degree = |i: usize| terms.adjacent[i].len();
    let pick = |keep: &dyn Fn(usize) -> bool| -> Vec<String> {
        (0..n)
            .filter(|&i| keep(i))
            .map(|i| terms.ids[i].clone())
            .collect()
    };
    TermDensity {
        total_terms: n,
        total_see_also,
        max_connections: (0..n).map(degree).max().unwrap_or(0),
        hub_terms: pick(&|i| {
            avg_connections.is_some_and(|avg| degree(i) as f64 > 2.0 * avg && degree(i) >= 3)
        }),
        isolated_terms: pick(&|i| degree(i) == 0),
        avg_connections,
    }
}
