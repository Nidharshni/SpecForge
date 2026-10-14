use crate::{non_empty, BEHAVIOR_KIND, INVARIANT_KIND};
use specforge_coverage as coverage;
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{PassDiagnostic, PassEntity, PassInput};

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
pub(crate) fn pass_coverage_tracking(input: &PassInput) -> Vec<PassDiagnostic> {
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
