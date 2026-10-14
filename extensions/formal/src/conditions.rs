use crate::{non_empty, BEHAVIOR_KIND, INVARIANT_KIND};
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::{PassDiagnostic, PassEntity, PassInput};

/// condition_check (Meyer's Design by Contract, RES-25 part I): a behavior
/// that obligates its callers (requires) must provide a benefit (ensures),
/// W096; a requires that names one condition twice repeats itself, W039; a
/// behavior that ensures without requiring anything is noted, I011;
/// an invariant whose guarantee has no machine-checkable `expression` is
/// prose only, W040.
pub(crate) fn pass_condition_check(input: &PassInput) -> Vec<PassDiagnostic> {
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
pub(crate) fn pass_analysis_available(input: &PassInput) -> Vec<PassDiagnostic> {
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
