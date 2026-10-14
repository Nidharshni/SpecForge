use super::{page_args, paged, table};
use crate::queries;
use serde::Serialize;
use specforge_extension_sdk::prelude::{CommandCall, CommandOutput, ContributionsBuilder};
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
    c.command("coverage_matrix", |cmd| {
        cmd.title("Show the persona coverage matrix")
            .description(
                "Per persona, by id, the features its journeys reach and the ones \
            they do not, with the coverage ratio and journey count, paged; \
            the feature total and the mean coverage over every persona.",
            )
            .category("query");
        page_args(cmd);
        cmd.handler(|call| {
            let matrix = queries::persona_coverage_matrix(call.graph());
            coverage(call, "personas", "persona", matrix, |e| {
                (&e.persona_id, &e.reach)
            })
        });
    });
    c.command("channel_coverage_matrix", |cmd| {
        cmd.title("Show the channel coverage matrix")
            .description(
                "Per channel, by id, the features the journeys using it reach and \
            the ones they do not, with the coverage ratio and journey count, \
            paged; the feature total and the mean coverage over every \
            channel.",
            )
            .category("query");
        page_args(cmd);
        cmd.handler(|call| {
            let matrix = queries::channel_coverage_matrix(call.graph());
            coverage(call, "channels", "channel", matrix, |e| {
                (&e.channel_id, &e.reach)
            })
        });
    });
    c.command("feature_overlap", |cmd| {
        cmd.title("Show features shared across deliverables")
            .description(
                "The features two or more deliverables reach through a journey or \
            a module, with those deliverables, most shared first, paged.",
            )
            .category("query");
        page_args(cmd);
        cmd.handler(|call| {
            let overlap = queries::feature_overlap(call.graph());
            let count = overlap.overlapping_features.len();
            let total_features = overlap.total_features;
            let extra = serde_json::json!({"count": count, "total_features": total_features});
            paged(
                call,
                "overlapping_features",
                overlap.overlapping_features,
                extra,
                |page, out| {
                    let rows: Vec<Vec<String>> = page
                        .items
                        .iter()
                        .map(|f| {
                            vec![
                                f.feature_id.clone(),
                                f.deliverable_count.to_string(),
                                f.deliverable_ids.join(", "),
                            ]
                        })
                        .collect();
                    out.push_str(&table(&["feature", "deliverables", "ids"], &rows));
                    let _ = writeln!(
                        out,
                        "{count} of {total_features} features shared by two or more \
            deliverables"
                    );
                },
            )
        });
    });
}

/// A coverage matrix command: one page of its entries (persona or
/// channel, as `kind` names them) under `key`, with the feature total and
/// the overall coverage; under `human` a table of each entry's counts and
/// ratio, then the overall coverage.
fn coverage<T: Serialize>(
    call: &CommandCall<'_>,
    key: &str,
    kind: &str,
    matrix: queries::CoverageMatrix<T>,
    reach: impl Fn(&T) -> (&String, &queries::Reach),
) -> CommandOutput {
    let extra = serde_json::json!({
        "total_features": matrix.total_features,
        "overall_coverage": matrix.overall_coverage,
    });
    let overall = matrix.overall_coverage;
    paged(call, key, matrix.entries, extra, |page, out| {
        let rows: Vec<Vec<String>> = page
            .items
            .iter()
            .map(|e| {
                let (id, r) = reach(e);
                vec![
                    id.clone(),
                    r.reachable_features.len().to_string(),
                    r.unreachable_features.len().to_string(),
                    format!("{:.0}%", r.coverage_ratio * 100.0),
                ]
            })
            .collect();
        out.push_str(&table(
            &[kind, "reachable", "unreachable", "coverage"],
            &rows,
        ));
        match overall {
            Some(overall) => {
                let _ = writeln!(out, "Overall coverage: {:.0}%", overall * 100.0);
            }
            None => {
                let _ = writeln!(out, "Overall coverage: - (no {key})");
            }
        }
    })
}
