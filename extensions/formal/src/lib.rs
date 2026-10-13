//! @specforge/formal — the formal-methods builtin, now authored with the
//! SpecForge extension SDK (SDK adoption; spec/extension-sdk docs).
//!
//! Its kinds, edges, enhancements and rules are declared with the SDK
//! builders in [`declaration`], its passes below; the handshake is derived
//! by the SDK from the extension metadata — contribution flags included.

mod declaration;

use specforge_coverage as coverage;
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{PassDiagnostic, PassEntity, PassInput};

#[specforge_extension_sdk::extension(
    name = "@specforge/formal",
    version = "1.0.0",
    description = "Formal methods: properties, axioms, protocols, refinements and processes, with contract, layering and coverage passes"
)]
struct Formal;

impl Contributions for Formal {
    fn contribute(c: &mut ContributionsBuilder) {
        // Diagrams (`model`, `outline`) draw the extension in this colour.
        c.theme_color("#9b59b6");
        c.peer("@specforge/software", "^1.0");

        // Declaration order is deliberately shuffled; the host must order
        // passes by the declared after-constraints, not by declaration.
        c.pass("event_graph_analyze", |p| {
            p.after("layering_verify").run(pass_event_graph_analyze);
        });
        c.pass("coverage_tracking", |p| {
            p.after("event_graph_analyze").run(pass_coverage_tracking);
        });
        c.pass("condition_check", |p| {
            p.after("resolve").run(pass_condition_check);
        });
        c.pass("layering_verify", |p| {
            p.after("condition_check").run(pass_layering_verify);
        });
        // The one check-phase pass: it runs with every compile, the four
        // above only under `specforge analyze`.
        c.pass("analysis_available", |p| {
            p.after("resolve")
                .phase("check")
                .run(pass_analysis_available);
        });

        declaration::declare(c);
    }
}

// ── Extension-owned compiler passes ────────────────────────────────────────
// Each pass is declared with its handler above: `__pass_<name>` receives the
// protocol's PassInput snapshot and answers its diagnostics (ADR 0013).

/// condition_check (Meyer's Design by Contract, RES-25 part I): a behavior
/// that obligates its callers (requires) must provide a benefit (ensures),
/// W096; a requires that names one condition twice repeats itself, W039; a
/// behavior that ensures without requiring anything is noted, I011;
/// an invariant whose guarantee has no machine-checkable `expression` is
/// prose only, W040.
fn pass_condition_check(input: &PassInput) -> Vec<PassDiagnostic> {
    let mut findings = Vec::new();
    for entity in &input.entities {
        match entity.kind.as_str() {
            BEHAVIOR_KIND => {
                let has_requires = non_empty(entity, "requires");
                let has_ensures = non_empty(entity, "ensures");
                if has_requires && !has_ensures {
                    findings.push(
                        PassDiagnostic::warning(
                            "W096",
                            format!("behavior '{}' declares requires but no ensures", entity.id),
                        )
                        .with_suggestion(
                            "add an ensures clause: a caller's obligation must buy a guarantee",
                        ),
                    );
                }
                for name in repeated_names(entity, "requires") {
                    findings.push(
                        PassDiagnostic::warning(
                            "W039",
                            format!(
                                "behavior '{}' requires '{name}' more than once (the repeat is redundant)",
                                entity.id
                            ),
                        )
                        .with_entity(&entity.id)
                        .with_suggestion(format!("remove the repeated '{name}' condition")),
                    );
                }
                if has_ensures && !has_requires {
                    findings.push(
                        PassDiagnostic::new(
                            "I011",
                            PassSeverity::Info,
                            format!("behavior '{}' declares ensures but no requires", entity.id),
                        )
                        .with_entity(&entity.id)
                        .with_suggestion(
                            "add a requires clause naming what callers must establish first, or leave it out if the behavior accepts every input",
                        ),
                    );
                }
            }
            INVARIANT_KIND
                if non_empty(entity, "guarantee") && !non_empty(entity, "expression") =>
            {
                findings.push(
                    PassDiagnostic::warning(
                        "W040",
                        format!(
                            "invariant '{}' states its guarantee in prose only (no expression)",
                            entity.id
                        ),
                    )
                    .with_entity(&entity.id)
                    .with_suggestion(
                        "add an `expression` stating the guarantee as a claim `specforge analyze --prove` can check",
                    ),
                );
            }
            _ => {}
        }
    }
    findings
}

/// The names `field` (a block's keys or a list's items, joined by `", "`)
/// lists more than once, sorted.
fn repeated_names<'a>(entity: &'a PassEntity, field: &str) -> Vec<&'a str> {
    let mut seen = std::collections::BTreeSet::new();
    let mut repeated = std::collections::BTreeSet::new();
    for name in entity
        .fields
        .get(field)
        .into_iter()
        .flat_map(|text| text.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if !seen.insert(name) {
            repeated.insert(name);
        }
    }
    repeated.into_iter().collect()
}

/// analysis_available (check phase): one note per compile, when behaviors
/// declare requires or ensures, that `specforge analyze` runs the formal
/// passes over them (I015).
fn pass_analysis_available(input: &PassInput) -> Vec<PassDiagnostic> {
    let conditioned = input
        .entities
        .iter()
        .filter(|e| {
            e.kind == BEHAVIOR_KIND && (non_empty(e, "requires") || non_empty(e, "ensures"))
        })
        .count();
    if conditioned == 0 {
        return Vec::new();
    }
    vec![PassDiagnostic::new(
        "I015",
        PassSeverity::Info,
        format!("{conditioned} behavior(s) declare requires/ensures; `specforge analyze` checks them"),
    )
    .with_suggestion(
        "run `specforge analyze` for the formal passes (condition_check, layering_verify, event_graph_analyze, coverage_tracking)",
    )]
}

/// coverage_tracking (RES-25 part I): one aggregated warning per run
/// listing the coverage items the coverage rule does not hold proven, the
/// undischarged set of the discharge funnel (W035); one note per item whose
/// every obligation a passing recorded test names, naming those tests
/// (I008); and each behavior's specification depth (I014, see
/// [`depth_findings`]). Items are invariants and the entities that count
/// toward coverage (entities W004 exempts that declare nothing are not
/// items). "Proven" is @specforge/testing's rule (`specforge-coverage`, ADR
/// 0004 D2-f) over the recorded test results and the entailed claims in the
/// pass input, so W035 never disagrees with `specforge analyze coverage`.
/// Whether an entity is exempt is the host's call, read from the snapshot
/// (`PassEntity::owes_none`).
fn pass_coverage_tracking(input: &PassInput) -> Vec<PassDiagnostic> {
    let proved: std::collections::BTreeSet<&str> = input
        .proved_claims
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let mut undischarged: Vec<&str> = Vec::new();
    let mut findings = Vec::new();
    let mut proven: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for e in &input.entities {
        let entity = coverage::Entity {
            testable: e.testable || e.kind == INVARIANT_KIND,
            ..coverage::Entity::from(e)
        };
        if !entity.counts_toward_coverage() {
            continue;
        }
        let tests = input.test_results.as_ref().map_or(&[][..], |r| r.of(&e.id));
        if coverage::Verdict::of(&entity, tests, proved.contains(e.id.as_str())).is_proven() {
            proven.insert(e.id.as_str());
        } else {
            undischarged.push(e.id.as_str());
        }
        // I008: proven by tests alone, not by an entailed claim.
        if coverage::Verdict::of(&entity, tests, false).is_proven() {
            let mut names: Vec<&str> = tests
                .iter()
                .filter(|t| t.status == TestStatus::Passed)
                .filter(|t| {
                    t.verify
                        .as_ref()
                        .is_some_and(|v| e.verify_texts.contains(v))
                })
                .map(|t| t.name.as_str())
                .collect();
            names.sort_unstable();
            names.dedup();
            findings.push(
                PassDiagnostic::new(
                    "I008",
                    PassSeverity::Info,
                    format!(
                        "{} '{}': all {} obligation(s) proven by recorded test(s): {}",
                        e.kind,
                        e.id,
                        entity.obligations(),
                        preview(&names)
                    ),
                )
                .with_entity(&e.id),
            );
        }
    }
    findings.extend(depth_findings(input, &proven));

    if !undischarged.is_empty() {
        let message = format!(
            "{} coverage item(s) are not proven by a recorded test or an entailed claim: {}",
            undischarged.len(),
            preview(&undischarged)
        );
        findings.push(PassDiagnostic::warning("W035", message).with_suggestion(
            "link a test to each obligation by its text (`verify = \"...\"`) and run `specforge collect`; `specforge analyze coverage` lists what is unproven (A001, A015)",
        ));
    }
    findings
}

/// The first ten of `items`, joined, with how many more there are.
fn preview(items: &[&str]) -> String {
    const PREVIEW: usize = 10;
    let mut text = items
        .iter()
        .take(PREVIEW)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let rest = items.len().saturating_sub(PREVIEW);
    if rest > 0 {
        text.push_str(&format!(" … and {rest} more"));
    }
    text
}

/// A behavior's specification depth (`SpecificationDepthLevel`), a ladder:
/// each level holds the one below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Depth {
    /// No edge in or out: the behavior is prose.
    Prose,
    /// It is connected in the graph, but declares no requires or ensures.
    EntityGraph,
    /// It declares requires or ensures.
    Conditions,
    /// Conditions, and it names invariants (`maintains` or `invariants`).
    Invariants,
    /// Invariants, and the coverage rule holds it proven.
    Proofs,
}

impl Depth {
    fn of(entity: &PassEntity, proven: bool) -> Self {
        if !(non_empty(entity, "requires") || non_empty(entity, "ensures")) {
            return if entity.incoming_edge_count + entity.outgoing_edge_count == 0 {
                Depth::Prose
            } else {
                Depth::EntityGraph
            };
        }
        if !(non_empty(entity, "maintains") || non_empty(entity, "invariants")) {
            return Depth::Conditions;
        }
        if proven {
            Depth::Proofs
        } else {
            Depth::Invariants
        }
    }

    fn name(self) -> &'static str {
        match self {
            Depth::Prose => "prose",
            Depth::EntityGraph => "entity_graph",
            Depth::Conditions => "conditions",
            Depth::Invariants => "invariants",
            Depth::Proofs => "proofs",
        }
    }

    /// What reaches the next level, from a level that is reported.
    fn next_step(self) -> Option<&'static str> {
        match self {
            Depth::Conditions => Some(
                "name the invariants it keeps (`maintains` or `invariants`) to reach level 3 (invariants)",
            ),
            Depth::Invariants => Some(
                "prove every obligation with a recorded test (`specforge collect`) to reach level 4 (proofs)",
            ),
            _ => None,
        }
    }
}

/// More behaviors than this below `conditions` earn the adoption note.
const ADOPTION_THRESHOLD: usize = 5;

/// I014: each behavior at `conditions` or deeper, with its level and the
/// step to the next; and, when more than [`ADOPTION_THRESHOLD`] behaviors
/// sit at `prose` or `entity_graph`, one note suggesting requires/ensures.
fn depth_findings(
    input: &PassInput,
    proven: &std::collections::BTreeSet<&str>,
) -> Vec<PassDiagnostic> {
    let mut findings = Vec::new();
    let mut shallow = 0usize;
    for entity in input.entities.iter().filter(|e| e.kind == BEHAVIOR_KIND) {
        let depth = Depth::of(entity, proven.contains(entity.id.as_str()));
        if depth < Depth::Conditions {
            shallow += 1;
            continue;
        }
        let mut finding = PassDiagnostic::new(
            "I014",
            PassSeverity::Info,
            format!(
                "behavior '{}' is at specification depth '{}' (level {} of 4)",
                entity.id,
                depth.name(),
                depth as usize
            ),
        )
        .with_entity(&entity.id);
        if let Some(step) = depth.next_step() {
            finding = finding.with_suggestion(step);
        }
        findings.push(finding);
    }
    if shallow > ADOPTION_THRESHOLD {
        findings.push(
            PassDiagnostic::new(
                "I014",
                PassSeverity::Info,
                format!(
                    "{shallow} behavior(s) are at specification depth 'prose' or 'entity_graph' (no requires or ensures)"
                ),
            )
            .with_suggestion(
                "add requires/ensures to the critical behaviors to reach level 2 (conditions)",
            ),
        );
    }
    findings
}

/// The kind whose entities are coverage items whatever their kind's
/// testability.
const INVARIANT_KIND: &str = "invariant";

fn non_empty(entity: &PassEntity, field: &str) -> bool {
    entity
        .fields
        .get(field)
        .is_some_and(|v| !v.trim().is_empty())
}

/// layering_verify (RES-25 part I): refinement chains must stay acyclic
/// (E041) and shallow (W031 beyond depth 4), keep the abstract's ensures
/// (E031), and every abstract behavior needs a refinement (W030).
const MAX_LAYERING_DEPTH: usize = 4;
const BEHAVIOR_KIND: &str = "behavior";
const REFINEMENT_KIND: &str = "refinement";
// Graph edges are labelled with the field that declared them, not the
// describe_fields edge-type name (see specforge-resolver linker).
const REFINEMENT_CONCRETE_FIELD: &str = "concrete_entity";
const REFINEMENT_ABSTRACT_FIELD: &str = "abstract_entity";
const REFINES_FIELD: &str = "refines";
const ABSTRACT_FLAG_FIELD: &str = "abstract";

/// How a concrete -> abstract layering step was declared.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Declared<'a> {
    /// A `refinement` entity naming both behaviors.
    Entity(&'a str),
    /// A `refines` field on the concrete behavior.
    Field,
}

#[derive(Debug, Clone, Copy)]
struct LayeringStep<'a> {
    declared: Declared<'a>,
    concrete: &'a str,
    abstract_id: &'a str,
}

/// Every concrete -> abstract step, from `refinement` entities and from
/// `refines` fields on behaviors. When both declare the same pair the
/// refinement entity wins (it is the more explicit record). Sorted by pair.
fn layering_steps(input: &PassInput) -> Vec<LayeringStep<'_>> {
    use std::collections::{BTreeMap, HashMap};

    let kind_of: HashMap<&str, &str> = input
        .entities
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let mut concrete: BTreeMap<&str, &str> = BTreeMap::new();
    let mut abstract_of: BTreeMap<&str, &str> = BTreeMap::new();
    let mut steps: BTreeMap<(&str, &str), Declared> = BTreeMap::new();
    for edge in &input.edges {
        let (source, target) = (edge.source.as_str(), edge.target.as_str());
        match (kind_of.get(source).copied(), edge.label.as_str()) {
            (Some(REFINEMENT_KIND), REFINEMENT_CONCRETE_FIELD) => {
                concrete.insert(source, target);
            }
            (Some(REFINEMENT_KIND), REFINEMENT_ABSTRACT_FIELD) => {
                abstract_of.insert(source, target);
            }
            (Some(BEHAVIOR_KIND), REFINES_FIELD) => {
                steps.entry((source, target)).or_insert(Declared::Field);
            }
            _ => {}
        }
    }
    for (refinement, concrete) in concrete {
        if let Some(&abstract_id) = abstract_of.get(refinement) {
            steps
                .entry((concrete, abstract_id))
                .and_modify(|declared| {
                    if *declared == Declared::Field {
                        *declared = Declared::Entity(refinement);
                    }
                })
                .or_insert(Declared::Entity(refinement));
        }
    }
    steps
        .into_iter()
        .map(|((concrete, abstract_id), declared)| LayeringStep {
            declared,
            concrete,
            abstract_id,
        })
        .collect()
}

fn pass_layering_verify(input: &PassInput) -> Vec<PassDiagnostic> {
    use std::collections::{HashMap, HashSet};

    let by_id: HashMap<&str, &PassEntity> =
        input.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let is_abstract = |id: &str| {
        by_id
            .get(id)
            .and_then(|e| e.fields.get(ABSTRACT_FLAG_FIELD))
            .is_some_and(|v| v == "true")
    };
    let steps = layering_steps(input);
    // concrete behavior -> the abstract behaviors it refines
    let mut refines: HashMap<&str, Vec<&str>> = HashMap::new();
    for step in &steps {
        if by_id.contains_key(step.concrete) && by_id.contains_key(step.abstract_id) {
            refines
                .entry(step.concrete)
                .or_default()
                .push(step.abstract_id);
        }
    }

    let mut findings = Vec::new();
    // Cycle detection over the refinement DAG (E041).
    let mut state: HashMap<&str, u8> = HashMap::new(); // 0 = visiting, 1 = done
    fn visit<'a>(
        id: &'a str,
        refines: &HashMap<&'a str, Vec<&'a str>>,
        state: &mut HashMap<&'a str, u8>,
        findings: &mut Vec<PassDiagnostic>,
    ) {
        match state.get(id) {
            Some(1) => return,
            Some(0) => {
                findings.push(PassDiagnostic::new(
                    "E041",
                    PassSeverity::Error,
                    format!("refinement chain cycle through '{}'", id),
                ));
                return;
            }
            _ => {}
        }
        state.insert(id, 0);
        if let Some(targets) = refines.get(id) {
            for target in targets.clone() {
                visit(target, refines, state, findings);
            }
        }
        state.insert(id, 1);
    }
    for id in refines.keys().copied().collect::<Vec<_>>() {
        visit(id, &refines, &mut state, &mut findings);
    }

    // Depth accounting (W031): chain depth beyond MAX_LAYERING_DEPTH. Walks
    // that revisit a node are cycles — already reported as E041, so they are
    // skipped here rather than double-reported as depth violations.
    let mut depth: HashMap<&str, usize> = HashMap::new();
    for id in refines.keys() {
        let mut current = *id;
        let mut steps = 0usize;
        let mut walked: HashMap<&str, ()> = HashMap::new();
        let mut cyclic = false;
        loop {
            if walked.insert(current, ()).is_some() {
                cyclic = true;
                break;
            }
            match refines.get(current).and_then(|t| t.first()) {
                Some(&next) if !depth.contains_key(&next) => {
                    current = next;
                    steps += 1;
                    if steps > MAX_LAYERING_DEPTH {
                        break;
                    }
                }
                Some(&next) => {
                    steps += depth[&next] + 1;
                    break;
                }
                None => break,
            }
        }
        if !cyclic {
            depth.insert(id, steps);
        }
    }
    for (id, d) in &depth {
        if *d > MAX_LAYERING_DEPTH {
            let message = if let Some(e) = by_id.get(id) {
                format!(
                    "behavior '{}' sits in a refinement chain {} layers deep (max {})",
                    e.id, d, MAX_LAYERING_DEPTH
                )
            } else {
                continue;
            };
            findings.push(PassDiagnostic::warning("W031", message).with_suggestion(
                "split the refinement chain or collapse intermediate abstractions",
            ));
        }
    }
    // E031 (C10-02): named-condition set inclusion. For each concrete
    // entity refining an abstract one, the concrete ensures names must be a
    // superset of the abstract's (pattern-catalog fa_pattern_e031):
    // strengthening is allowed, weakening is the layering violation.
    let ensures_names = |id: &str| -> Option<Vec<String>> {
        let entity = by_id.get(id)?;
        let raw = entity.fields.get("ensures")?;
        let names: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if names.is_empty() {
            None
        } else {
            Some(names)
        }
    };
    for step in &steps {
        let LayeringStep {
            declared,
            concrete,
            abstract_id,
        } = *step;
        let (Some(concrete_names), Some(abstract_names)) =
            (ensures_names(concrete), ensures_names(abstract_id))
        else {
            continue;
        };
        let missing: Vec<&str> = abstract_names
            .iter()
            .filter(|name| !concrete_names.contains(name))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            let names_list = missing.join(", ");
            let message = match declared {
                Declared::Entity(refinement) => format!(
                    "refinement '{refinement}': '{concrete}' drops ensures condition(s) [{names_list}] from abstract '{abstract_id}'"
                ),
                Declared::Field => format!(
                    "'{concrete}' refines '{abstract_id}' but drops its ensures condition(s) [{names_list}]"
                ),
            };
            findings.push(PassDiagnostic::new("E031", PassSeverity::Error, message).with_suggestion(
                "keep every abstract ensures condition in the refinement (strengthening is allowed; weakening is not)",
            ));
        }
    }

    // W110: the `refines` field declares layering against an abstraction,
    // so its target must be marked `abstract true`. Refinement entities
    // predate the flag and may name any behavior.
    for step in steps.iter().filter(|s| s.declared == Declared::Field) {
        if by_id.contains_key(step.abstract_id) && !is_abstract(step.abstract_id) {
            findings.push(
                PassDiagnostic::warning(
                    "W110",
                    format!(
                        "'{}' refines '{}', which is not marked `abstract true`",
                        step.concrete, step.abstract_id
                    ),
                )
                .with_suggestion(format!(
                    "add `abstract true` to '{}', or refine an abstract behavior",
                    step.abstract_id
                )),
            );
        }
    }

    // W030: an abstract behavior nothing refines is an incomplete layer.
    let refined: HashSet<&str> = steps.iter().map(|s| s.abstract_id).collect();
    for entity in input.entities.iter().filter(|e| {
        e.kind == BEHAVIOR_KIND && is_abstract(&e.id) && !refined.contains(e.id.as_str())
    }) {
        findings.push(
            PassDiagnostic::warning(
                "W030",
                format!("abstract behavior '{}' has no concrete refinement", entity.id),
            )
            .with_suggestion(format!(
                "add a behavior with `refines {}`, or a `refinement` entity naming it as abstract_entity",
                entity.id
            )),
        );
    }

    // Deterministic order: DFS seeds came from a HashMap, so sort by
    // (entity, code) before returning (hardening-plan D4 / R-6).
    findings.sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message)));
    // Two refinements naming the same concrete/abstract pair make the DFS
    // revisit the same back edge — dedup identical diagnostics (C10-11).
    findings.dedup_by(|a, b| a.code == b.code && a.message == b.message);
    findings
}

/// event_graph_analyze (RES-25 part I): every produced event should have a
/// consumer (W029).
fn pass_event_graph_analyze(input: &PassInput) -> Vec<PassDiagnostic> {
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

#[cfg(test)]
mod pass_tests;

#[cfg(test)]
mod coverage_tracking_tests;

specforge_extension_sdk::component_guest!(build = specforge_extension_build);

// -- C10-00/C10-11: process semantics + detector soundness slivers --

#[cfg(test)]
mod process_tests;

// -- ADR 0040: the formal diagnostics the specs name --

#[cfg(test)]
mod formal_diagnostics_tests;
