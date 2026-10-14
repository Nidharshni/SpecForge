use super::{entity_arg, lookup};
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
    c.command("milestone_completion", |cmd| {
        cmd.title("Show milestone completion progress")
            .description(
                "How many of a milestone's features are done: the count, the \
            ratio in [0, 1] and their ids; beside it, how many the project's \
            recorded tests prove (each feature's implementing behaviors, \
            after `specforge collect`).",
            )
            .category("query");
        entity_arg(cmd, "milestone");
        cmd.handler(|call| {
            lookup(
                call,
                "milestone",
                |graph, id| {
                    queries::milestone_completion(graph, id)
                        .map(|r| r.with_evidence(graph, call.evidence()))
                },
                |r, out| {
                    let _ = writeln!(
                        out,
                        "Milestone: {} ({})",
                        r.milestone_id,
                        r.status.as_deref().unwrap_or("-")
                    );
                    let _ = writeln!(
                        out,
                        "Completion: {:.0}% ({}/{} features done)",
                        r.completion_ratio * 100.0,
                        r.done_count,
                        r.total_features
                    );
                    match (&r.proven_count, &r.proven_ratio) {
                        (Some(count), Some(ratio)) => {
                            let _ = writeln!(
                                out,
                                "Evidence:   {:.0}% ({count}/{} features proven by recorded tests)",
                                ratio * 100.0,
                                r.total_features
                            );
                        }
                        _ => {
                            let state = r.evidence.as_ref().map_or_else(
                                || "none recorded".to_string(),
                                |e| e.describe(),
                            );
                            let _ = writeln!(out, "Evidence:   {state}");
                        }
                    }
                    let evidence = r.feature_evidence.as_deref().unwrap_or_default();
                    for f in &r.features {
                        let status = f.status.as_deref().unwrap_or("-");
                        match evidence.iter().find(|e| e.feature_id == f.id) {
                            Some(e) if e.behaviors == 0 => {
                                let _ = writeln!(out, "  {} [{status}] no implementing behavior", f.id);
                            }
                            Some(e) => {
                                let _ = writeln!(
                                    out,
                                    "  {} [{status}] {} {}/{} behaviors proven ({}/{} obligations{})",
                                    f.id,
                                    if e.proven { "proven" } else { "unproven" },
                                    e.proven_behaviors,
                                    e.behaviors,
                                    e.proven_obligations,
                                    e.obligations,
                                    if e.failing > 0 {
                                        format!(", {} failing", e.failing)
                                    } else {
                                        String::new()
                                    }
                                );
                            }
                            None => {
                                let _ = writeln!(out, "  {} [{status}]", f.id);
                            }
                        }
                    }
                },
            )
        });
    });
    c.command("journey_coverage", |cmd| {
        cmd.title("Show journey feature-module coverage")
            .description(
                "How many of a journey's features are done, and the ones that are \
            not.",
            )
            .category("query");
        entity_arg(cmd, "journey");
        cmd.handler(|call| {
            lookup(call, "journey", queries::journey_coverage, |r, out| {
                let _ = writeln!(
                    out,
                    "Journey: {} (persona: {})",
                    r.journey_id,
                    r.persona.as_deref().unwrap_or("-")
                );
                let pct = if r.total_features > 0 {
                    r.covered_count as f64 / r.total_features as f64 * 100.0
                } else {
                    0.0
                };
                let _ = writeln!(
                    out,
                    "Coverage: {pct:.0}% ({}/{} features done)",
                    r.covered_count, r.total_features
                );
                if !r.uncovered_features.is_empty() {
                    let _ = writeln!(out, "Uncovered:");
                    for f in &r.uncovered_features {
                        let _ = writeln!(out, "  {f}");
                    }
                }
            })
        });
    });
}
