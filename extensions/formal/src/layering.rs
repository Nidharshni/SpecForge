use crate::BEHAVIOR_KIND;
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{PassDiagnostic, PassEntity, PassInput};

/// layering_verify (RES-25 part I): refinement chains must stay acyclic
/// (E041) and shallow (W031 beyond depth 4), keep the abstract's ensures
/// (E031), and every abstract behavior needs a refinement (W030).
const MAX_LAYERING_DEPTH: usize = 4;

const REFINEMENT_KIND: &str = "refinement";

// Graph edges are labelled with the field that declared them, not the
// describe_fields edge-type name (see specforge-resolver linker).
pub(crate) const REFINEMENT_CONCRETE_FIELD: &str = "concrete_entity";

pub(crate) const REFINEMENT_ABSTRACT_FIELD: &str = "abstract_entity";

pub(crate) const REFINES_FIELD: &str = "refines";

pub(crate) const ABSTRACT_FLAG_FIELD: &str = "abstract";

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

pub(crate) fn pass_layering_verify(input: &PassInput) -> Vec<PassDiagnostic> {
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
