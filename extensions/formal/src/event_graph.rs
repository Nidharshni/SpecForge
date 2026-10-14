use crate::{non_empty, BEHAVIOR_KIND};
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{PassDiagnostic, PassEntity, PassInput};

/// event_graph_analyze (RES-25 part I): every produced event should have a
/// consumer (W029).
pub(crate) fn pass_event_graph_analyze(input: &PassInput) -> Vec<PassDiagnostic> {
    use std::collections::HashMap;

    let by_id: HashMap<&str, &PassEntity> =
        input.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut produced: HashMap<&str, usize> = HashMap::new();
    let mut consumed: HashMap<&str, usize> = HashMap::new();
    for edge in &input.edges {
        match edge.label.as_str() {
            "produces" => *produced.entry(edge.target.as_str()).or_default() += 1,
            "consumes" => *consumed.entry(edge.target.as_str()).or_default() += 1,
            _ => {}
        }
    }

    let mut findings: Vec<PassDiagnostic> = Vec::new();
    // Process semantics (C10-00): an event participating in a process is
    // used by it (suppresses W029), and ProcessComposesProcess edges form a
    // composition graph whose cycles are the documented E042.
    let mut process_edges: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    for edge in &input.edges {
        // The host labels these edges with the FIELD name (an event's
        // participates_in, a process's sub_processes); the edge-type names
        // (EventParticipatesInProcess / ProcessComposesProcess) are accepted
        // too.
        match edge.label.as_str() {
            "EventParticipatesInProcess" | "participates_in" => {
                *consumed.entry(edge.source.as_str()).or_default() += 1;
            }
            "ProcessComposesProcess" | "sub_processes" => {
                process_edges
                    .entry(edge.source.as_str())
                    .or_default()
                    .insert(edge.target.as_str());
            }
            _ => {}
        }
    }

    // Process composition cycles (E042): exact-membership DFS over the
    // composition graph, deterministic order (sorted seeds + sorted edges).
    {
        let mut seeds: Vec<&str> = process_edges.keys().copied().collect();
        seeds.sort();
        let mut color: HashMap<&str, u8> = process_edges.keys().map(|&k| (k, 0u8)).collect();
        let mut path: Vec<&str> = Vec::new();
        fn dfs<'a>(
            node: &'a str,
            adj: &std::collections::BTreeMap<&'a str, std::collections::BTreeSet<&'a str>>,
            color: &mut HashMap<&'a str, u8>,
            path: &mut Vec<&'a str>,
            by_id: &HashMap<&'a str, &'a PassEntity>,
            findings: &mut Vec<PassDiagnostic>,
        ) {
            color.insert(node, 1);
            path.push(node);
            if let Some(neighbors) = adj.get(node) {
                for &next in neighbors {
                    match color.get(next).copied().unwrap_or(0) {
                        1 => {
                            if let Some(pos) = path.iter().position(|&n| n == next) {
                                let cycle: Vec<&str> = path[pos..].to_vec();
                                let rendered = cycle.join(" -> ");
                                let first = by_id.get(cycle[0]).copied();
                                findings.push(
                                    PassDiagnostic::new(
                                        "E042",
                                        PassSeverity::Error,
                                        format!(
                                            "process composition cycle: {rendered} -> {next}"
                                        ),
                                    )
                                    .with_span(PassSpan {
                                        file: first
                                            .and_then(|e| e.span.as_ref())
                                            .map(|s| s.file.clone())
                                            .unwrap_or_default(),
                                        start_line: first
                                            .and_then(|e| e.span.as_ref())
                                            .map(|s| s.start_line)
                                            .unwrap_or(0),
                                        start_col: first
                                            .and_then(|e| e.span.as_ref())
                                            .map(|s| s.start_col)
                                            .unwrap_or(0),
                                        end_line: first
                                            .and_then(|e| e.span.as_ref())
                                            .map(|s| s.end_line)
                                            .unwrap_or(0),
                                        end_col: first
                                            .and_then(|e| e.span.as_ref())
                                            .map(|s| s.end_col)
                                            .unwrap_or(0),
                                    })
                                    .with_suggestion(
                                        "break the composition cycle — a process cannot compose (transitively) with itself",
                                    ),
                                );
                            }
                        }
                        0 => dfs(next, adj, color, path, by_id, findings),
                        _ => {}
                    }
                }
            }
            path.pop();
            color.insert(node, 2);
        }
        for &seed in &seeds {
            if color.get(&seed).copied() == Some(0) {
                dfs(
                    seed,
                    &process_edges,
                    &mut color,
                    &mut path,
                    &by_id,
                    &mut findings,
                );
            }
        }
    }

    for (id, producers) in &produced {
        if consumed.contains_key(id) {
            continue;
        }
        let Some(entity) = by_id.get(id) else {
            continue;
        };
        if entity.kind != "event" {
            continue;
        }
        findings.push(
            PassDiagnostic::warning(
                "W029",
                format!(
                    "event '{}' is produced ({}x) but never consumed",
                    id, producers
                ),
            )
            .with_span(PassSpan {
                file: entity
                    .span
                    .as_ref()
                    .map(|s| s.file.clone())
                    .unwrap_or_default(),
                start_line: entity.span.as_ref().map(|s| s.start_line).unwrap_or(0),
                start_col: entity.span.as_ref().map(|s| s.start_col).unwrap_or(0),
                end_line: entity.span.as_ref().map(|s| s.end_line).unwrap_or(0),
                end_col: entity.span.as_ref().map(|s| s.end_col).unwrap_or(0),
            })
            .with_suggestion(
                "add a behavior that consumes the event, or drop the produces reference",
            ),
        );
    }
    findings.extend(event_flow_findings(input, &by_id));
    findings.extend(port_connectivity_findings(input, &by_id));
    // Deterministic order: produced-events came from a HashMap (hardening-plan
    // D4 / R-6).
    findings.sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message)));
    findings
}

const EVENT_KIND: &str = "event";

const PORT_KIND: &str = "port";

/// The constraints a behavior or event declares on its synchronization
/// (formal's `sync` enhancement): the one mitigation the data carries.
const SYNC_FIELD: &str = "sync";

/// The event flow graph: behavior -> event for each `produces`, event ->
/// behavior for each `consumes` (the behavior reacts to the event).
#[derive(Default)]
struct EventFlow<'a> {
    next: std::collections::BTreeMap<&'a str, std::collections::BTreeSet<&'a str>>,
    /// behavior -> the events it produces / consumes.
    produces: std::collections::BTreeMap<&'a str, std::collections::BTreeSet<&'a str>>,
    consumes: std::collections::BTreeMap<&'a str, std::collections::BTreeSet<&'a str>>,
}

impl<'a> EventFlow<'a> {
    fn of(input: &'a PassInput, by_id: &std::collections::HashMap<&str, &PassEntity>) -> Self {
        let kind = |id: &str| by_id.get(id).map(|e| e.kind.as_str());
        let mut flow = EventFlow::default();
        for edge in &input.edges {
            let (behavior, event) = (edge.source.as_str(), edge.target.as_str());
            if kind(behavior) != Some(BEHAVIOR_KIND) || kind(event) != Some(EVENT_KIND) {
                continue;
            }
            match edge.label.as_str() {
                "produces" => {
                    flow.next.entry(behavior).or_default().insert(event);
                    flow.produces.entry(behavior).or_default().insert(event);
                }
                "consumes" => {
                    flow.next.entry(event).or_default().insert(behavior);
                    flow.consumes.entry(behavior).or_default().insert(event);
                }
                _ => {}
            }
        }
        flow
    }

    fn nodes(&self) -> std::collections::BTreeSet<&'a str> {
        self.next
            .iter()
            .flat_map(|(from, to)| std::iter::once(*from).chain(to.iter().copied()))
            .collect()
    }

    /// The strongly connected components (Tarjan), each sorted, in a
    /// deterministic order.
    fn components(&self) -> Vec<Vec<&'a str>> {
        struct Tarjan<'g, 'a> {
            flow: &'g EventFlow<'a>,
            index: std::collections::HashMap<&'a str, usize>,
            low: std::collections::HashMap<&'a str, usize>,
            stack: Vec<&'a str>,
            on_stack: std::collections::HashSet<&'a str>,
            components: Vec<Vec<&'a str>>,
        }
        impl<'a> Tarjan<'_, 'a> {
            fn visit(&mut self, node: &'a str) {
                let n = self.index.len();
                self.index.insert(node, n);
                self.low.insert(node, n);
                self.stack.push(node);
                self.on_stack.insert(node);
                let flow = self.flow;
                for &next in flow.next.get(node).into_iter().flatten() {
                    if !self.index.contains_key(next) {
                        self.visit(next);
                        let low = self.low[node].min(self.low[next]);
                        self.low.insert(node, low);
                    } else if self.on_stack.contains(next) {
                        let low = self.low[node].min(self.index[next]);
                        self.low.insert(node, low);
                    }
                }
                if self.low[node] == self.index[node] {
                    let mut component = Vec::new();
                    while let Some(member) = self.stack.pop() {
                        self.on_stack.remove(member);
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    component.sort_unstable();
                    self.components.push(component);
                }
            }
        }
        let mut tarjan = Tarjan {
            flow: self,
            index: Default::default(),
            low: Default::default(),
            stack: Vec::new(),
            on_stack: Default::default(),
            components: Vec::new(),
        };
        for node in self.nodes() {
            if !tarjan.index.contains_key(node) {
                tarjan.visit(node);
            }
        }
        tarjan.components
    }

    /// A shortest cycle from `start` back to itself inside `component`.
    fn cycle_through(&self, start: &'a str, component: &[&'a str]) -> Vec<&'a str> {
        let inside: std::collections::HashSet<&str> = component.iter().copied().collect();
        let mut parent: std::collections::HashMap<&'a str, &'a str> = Default::default();
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for &next in self.next.get(node).into_iter().flatten() {
                if !inside.contains(next) {
                    continue;
                }
                if next == start {
                    let mut path = vec![start, node];
                    let mut at = node;
                    while at != start {
                        at = parent[at];
                        path.push(at);
                    }
                    path.reverse();
                    return path;
                }
                if next != start && !parent.contains_key(next) {
                    parent.insert(next, node);
                    queue.push_back(next);
                }
            }
        }
        vec![start]
    }
}

/// Event flow (RES-25 part I, structural only): an event cycle through two
/// or more behaviors with no `sync` on any member (E034); a behavior that
/// consumes an event it also produces, with no `sync` on either (W032); a
/// produced event that declares no `sync`, so nothing bounds its channel
/// (W034); and, when the graph has flow edges and neither cycle is found,
/// one I009.
fn event_flow_findings(
    input: &PassInput,
    by_id: &std::collections::HashMap<&str, &PassEntity>,
) -> Vec<PassDiagnostic> {
    let flow = EventFlow::of(input, by_id);
    let synced = |id: &str| by_id.get(id).is_some_and(|e| non_empty(e, SYNC_FIELD));
    let is_behavior = |id: &str| by_id.get(id).is_some_and(|e| e.kind == BEHAVIOR_KIND);
    let mut findings = Vec::new();

    let mut cycles = 0usize;
    for component in flow.components() {
        let behaviors: Vec<&str> = component
            .iter()
            .copied()
            .filter(|id| is_behavior(id))
            .collect();
        if behaviors.len() < 2 || component.iter().any(|id| synced(id)) {
            continue;
        }
        cycles += 1;
        let path = flow.cycle_through(behaviors[0], &component);
        findings.push(
            PassDiagnostic::new(
                "E034",
                PassSeverity::Error,
                format!(
                    "unmitigated event cycle: {} (no behavior or event in it declares sync)",
                    path.join(" -> ")
                ),
            )
            .with_entity(behaviors[0])
            .with_suggestion(format!(
                "declare `sync` (a timeout, barrier or delivery bound) on one of {}, or break the cycle",
                component.join(", ")
            )),
        );
    }

    for (&behavior, produced) in &flow.produces {
        let Some(consumed) = flow.consumes.get(behavior) else {
            continue;
        };
        for &event in produced.intersection(consumed) {
            if synced(behavior) || synced(event) {
                continue;
            }
            cycles += 1;
            findings.push(
                PassDiagnostic::warning(
                    "W032",
                    format!(
                        "behavior '{behavior}' consumes event '{event}' and produces it again, with no sync on either (an unmitigated retry cycle)"
                    ),
                )
                .with_entity(behavior)
                .with_suggestion(format!(
                    "declare `sync` (a timeout or backoff) on '{event}' or '{behavior}'"
                )),
            );
        }
    }

    let produced: std::collections::BTreeSet<&str> =
        flow.produces.values().flatten().copied().collect();
    for &event in produced.iter().filter(|&&e| !synced(e)) {
        findings.push(
            PassDiagnostic::warning(
                "W034",
                format!("event '{event}' is produced but declares no sync, so nothing bounds its channel"),
            )
            .with_entity(event)
            .with_suggestion("declare `sync` on the event: a timeout, a buffer limit or its delivery semantics"),
        );
    }

    if cycles == 0 && !flow.next.is_empty() {
        let nodes = flow.nodes();
        let behaviors = nodes.iter().filter(|id| is_behavior(id)).count();
        findings.push(PassDiagnostic::new(
            "I009",
            PassSeverity::Info,
            format!(
                "no unmitigated cycle in the event graph ({behaviors} behavior(s), {} event(s)); structural only, runtime conditions are not analyzed",
                nodes.len() - behaviors
            ),
        ));
    }
    findings
}

/// The edge-count ratio beyond which a port's access is unbalanced (W033).
const CONNECTIVITY_RATIO: usize = 3;

/// W033 (a structural hint, not a fairness guarantee): a port that two or
/// more behaviors use (`ports`) where the most-referenced of them has more
/// than three times the incoming edges of the least-referenced, a behavior
/// nothing references counting as one.
fn port_connectivity_findings(
    input: &PassInput,
    by_id: &std::collections::HashMap<&str, &PassEntity>,
) -> Vec<PassDiagnostic> {
    let kind = |id: &str| by_id.get(id).map(|e| e.kind.as_str());
    let mut users: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        Default::default();
    for edge in input.edges.iter().filter(|e| e.label == "ports") {
        if kind(&edge.source) == Some(BEHAVIOR_KIND) && kind(&edge.target) == Some(PORT_KIND) {
            users
                .entry(edge.target.as_str())
                .or_default()
                .insert(edge.source.as_str());
        }
    }
    let mut findings = Vec::new();
    for (port, behaviors) in users.iter().filter(|(_, b)| b.len() > 1) {
        let incoming = |id: &&str| by_id[*id].incoming_edge_count;
        // Ties keep the first id, so the pair named is deterministic.
        let most = behaviors
            .iter()
            .copied()
            .fold(None::<&str>, |best, id| match best {
                Some(b) if incoming(&b) >= incoming(&id) => Some(b),
                _ => Some(id),
            });
        let least = behaviors
            .iter()
            .copied()
            .fold(None::<&str>, |best, id| match best {
                Some(b) if incoming(&b) <= incoming(&id) => Some(b),
                _ => Some(id),
            });
        let (Some(most), Some(least)) = (most, least) else {
            continue;
        };
        let (high, low) = (incoming(&most), incoming(&least));
        // A behavior nothing references counts as one edge, so the ratio is
        // never one over zero.
        if high > CONNECTIVITY_RATIO * low.max(1) {
            findings.push(
                PassDiagnostic::warning(
                    "W033",
                    format!(
                        "port '{port}' is used by {} behaviors with unbalanced connectivity: '{most}' has {high} incoming edge(s), '{least}' {low} (more than {CONNECTIVITY_RATIO}:1)",
                        behaviors.len()
                    ),
                )
                .with_entity(*port)
                .with_suggestion(
                    "review how the behaviors share the port: a structural hint that one consumer dominates its access, not a fairness check",
                ),
            );
        }
    }
    findings
}
