//! Which nodes of a directed graph lie on a cycle, and one closed walk
//! through each cycle (ADR 0196). W061, W113 and every `cycle_detection`
//! rule read it; the answer depends only on the edges, never on the order
//! they were written or inserted in.
//!
//! Std-only, so the extension guests link it as the host does.
//!
//! NOT consolidated here: extension peer-dependency cycles (E027), which
//! follow the install order.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The nodes of one strongly connected component that holds a cycle: two
/// or more nodes that reach one another, or one node with an edge to itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle<N> {
    /// Sorted, each once.
    pub members: Vec<N>,
    /// A closed walk over real edges that visits every member: it starts and
    /// ends at `members[0]`, and each step goes to the nearest member not yet
    /// walked (successors in order), then back to the start. A member may
    /// appear more than once; a simple cycle is walked once round.
    pub path: Vec<N>,
}

/// The cycles of `edges` (each node -> its successors), in order of their
/// smallest member. A node that only appears as a target is a node with no
/// successors. Members of different cycles are disjoint.
pub fn cycles<N: Ord + Clone>(edges: &BTreeMap<N, BTreeSet<N>>) -> Vec<Cycle<N>> {
    let mut nodes: BTreeSet<&N> = edges.keys().collect();
    nodes.extend(edges.values().flatten());
    let nodes: Vec<&N> = nodes.into_iter().collect();
    let index: BTreeMap<&N, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    // Nodes are indexed in sorted order, so an index orders as its node does.
    let succ: Vec<Vec<usize>> = nodes
        .iter()
        .map(|n| {
            edges
                .get(*n)
                .into_iter()
                .flatten()
                .map(|t| index[t])
                .collect()
        })
        .collect();
    let name = |i: &usize| nodes[*i].clone();
    components(&succ)
        .into_iter()
        .map(|members| Cycle {
            path: walk(&members, &succ).iter().map(name).collect(),
            members: members.iter().map(name).collect(),
        })
        .collect()
}

/// The strongly connected components that hold a cycle (iterative Tarjan,
/// O(V + E)), each sorted, in order of their smallest node.
fn components(succ: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let n = succ.len();
    let (mut order, mut low) = (vec![UNSEEN; n], vec![0; n]);
    let mut on_stack = vec![false; n];
    let (mut stack, mut next) = (Vec::new(), 0);
    let mut found: Vec<Vec<usize>> = Vec::new();
    for root in 0..n {
        if order[root] != UNSEEN {
            continue;
        }
        order[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        // (node, index of the next successor to look at)
        let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&(v, i)) = calls.last() {
            if let Some(&w) = succ[v].get(i) {
                if let Some(call) = calls.last_mut() {
                    call.1 += 1;
                }
                if order[w] == UNSEEN {
                    order[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    calls.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(order[w]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == order[v] {
                let mut component = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                if component.len() > 1 || succ[v].contains(&v) {
                    component.sort_unstable();
                    found.push(component);
                }
            }
        }
    }
    found.sort_unstable();
    found
}

/// A closed walk from the smallest member through every member: to the
/// nearest member not yet walked, then back.
fn walk(members: &[usize], succ: &[Vec<usize>]) -> Vec<usize> {
    let inside: BTreeSet<usize> = members.iter().copied().collect();
    let start = members[0];
    let mut unwalked = inside.clone();
    unwalked.remove(&start);
    let mut path = vec![start];
    let mut at = start;
    while !unwalked.is_empty() {
        let leg = shortest(at, |n| unwalked.contains(&n), &inside, succ);
        for n in &leg[1..] {
            unwalked.remove(n);
        }
        at = leg[leg.len() - 1];
        path.extend_from_slice(&leg[1..]);
    }
    let back = shortest(at, |n| n == start, &inside, succ);
    path.extend_from_slice(&back[1..]);
    path
}

/// The shortest path of one edge or more from `from` to a node `goal`
/// accepts, inside `inside`, successors in order.
fn shortest(
    from: usize,
    goal: impl Fn(usize) -> bool,
    inside: &BTreeSet<usize>,
    succ: &[Vec<usize>],
) -> Vec<usize> {
    let mut came: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue = VecDeque::from([from]);
    while let Some(v) = queue.pop_front() {
        for &w in &succ[v] {
            if !inside.contains(&w) || came.contains_key(&w) {
                continue;
            }
            came.insert(w, v);
            if goal(w) {
                let mut leg = vec![w];
                let mut at = w;
                loop {
                    at = came[&at];
                    leg.push(at);
                    if at == from {
                        break;
                    }
                }
                leg.reverse();
                return leg;
            }
            queue.push_back(w);
        }
    }
    unreachable!("a strongly connected component reaches every member")
}
