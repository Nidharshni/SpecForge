use super::{entity_arg, lookup, page_args, paged, table};
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
    c.command("deliverable_completion", |cmd| {
        cmd.title("Show deliverable completion")
            .description(
                "How many of the milestones a deliverable is tracked by are \
            completed, as a count and a ratio in [0, 1]; asking for details adds each \
            milestone's feature completion.",
            )
            .category("query");
        entity_arg(cmd, "deliverable");
        cmd.arg("details", |a| {
            a.flag()
                .description("Include each milestone's feature completion");
        });
        cmd.handler(|call| {
            let details = call.flag("details");
            lookup(
                call,
                "deliverable",
                |g, id| queries::deliverable_completion(g, id, details),
                |r, out| {
                    let _ = writeln!(out, "Deliverable: {}", r.deliverable_id);
                    let _ = writeln!(
                        out,
                        "Completion: {:.0}% ({}/{} milestones completed)",
                        r.completion_ratio * 100.0,
                        r.completed_count,
                        r.milestone_count
                    );
                    if let Some(details) = &r.milestone_details {
                        let rows: Vec<Vec<String>> = details
                            .iter()
                            .map(|m| {
                                vec![
                                    m.milestone_id.clone(),
                                    m.status.clone().unwrap_or_else(|| "-".to_string()),
                                    format!("{}/{}", m.done_count, m.total_features),
                                    format!("{:.0}%", m.completion_ratio * 100.0),
                                ]
                            })
                            .collect();
                        out.push_str(&table(
                            &["milestone", "status", "features done", "completion"],
                            &rows,
                        ));
                    }
                },
            )
        });
    });
    c.command("release_completion", |cmd| {
        cmd.title("Show release completion")
            .description(
                "How many of a release's deliverables are shipped, as a count and \
            a ratio in [0, 1] (null without deliverables).",
            )
            .category("query");
        entity_arg(cmd, "release");
        cmd.handler(|call| {
            lookup(call, "release", queries::release_completion, |r, out| {
                let _ = writeln!(out, "Release: {}", r.release_id);
                match r.completion_ratio {
                    Some(ratio) => {
                        let _ = writeln!(
                            out,
                            "Completion: {:.0}% ({}/{} deliverables shipped)",
                            ratio * 100.0,
                            r.shipped,
                            r.total
                        );
                    }
                    None => {
                        let _ = writeln!(out, "Completion: - (no deliverables)");
                    }
                }
            })
        });
    });
    c.command("deliverable_priority", |cmd| {
        cmd.title("Show a deliverable's derived priority")
            .description(
                "The highest priority among the milestones and journeys a \
            deliverable references that declare one (null when none does), \
            and how many declare one.",
            )
            .category("query");
        entity_arg(cmd, "deliverable");
        cmd.handler(|call| {
            lookup(
                call,
                "deliverable",
                queries::deliverable_priority,
                |r, out| {
                    let _ = writeln!(
                        out,
                        "Deliverable '{}' priority: {} (from {} prioritized milestones \
            and journeys)",
                        r.deliverable_id,
                        r.priority.as_deref().unwrap_or("none"),
                        r.source_count
                    );
                },
            )
        });
    });
    c.command("unscheduled_features", |cmd| {
        cmd.title("Show features no milestone schedules")
            .description(
                "The features no milestone lists, sorted by id, with how many \
            features there are and how many are scheduled.",
            )
            .category("query");

        cmd.handler(|call| {
            let result = queries::unscheduled_features(call.graph());
            call.render(&result, |out| {
                let rows: Vec<Vec<String>> = result
                    .features
                    .iter()
                    .zip(&result.statuses)
                    .map(|(id, status)| {
                        vec![
                            id.clone(),
                            status.clone().unwrap_or_else(|| "-".to_string()),
                        ]
                    })
                    .collect();
                out.push_str(&table(&["id", "status"], &rows));
                let _ = writeln!(
                    out,
                    "{} of {} features unscheduled",
                    result.count, result.total_features
                );
            })
        });
    });
    c.command("owner_workload", |cmd| {
        cmd.title("Show ownership by owner")
            .description(
                "Per owner, the features, milestones, deliverables and releases \
            it owns (most first, ties by owner), paged, and how many of \
            those entities have no owner.",
            )
            .category("query");
        page_args(cmd);
        cmd.handler(|call| {
            let workload = queries::owner_workload(call.graph());
            let extra = serde_json::json!({
                "unowned_count": workload.unowned_count,
                "total_entities": workload.total_entities,
            });
            let (unowned, total) = (workload.unowned_count, workload.total_entities);
            paged(call, "owners", workload.owners, extra, |page, out| {
                let rows: Vec<Vec<String>> = page
                    .items
                    .iter()
                    .map(|o| {
                        vec![
                            o.owner.clone(),
                            o.entity_count.to_string(),
                            o.by_kind.features.to_string(),
                            o.by_kind.milestones.to_string(),
                            o.by_kind.deliverables.to_string(),
                            o.by_kind.releases.to_string(),
                        ]
                    })
                    .collect();
                out.push_str(&table(
                    &[
                        "owner",
                        "entities",
                        "features",
                        "milestones",
                        "deliverables",
                        "releases",
                    ],
                    &rows,
                ));
                let _ = writeln!(out, "Unowned: {unowned} of {total} entities");
            })
        });
    });
}
