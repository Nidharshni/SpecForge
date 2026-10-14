use super::{entity_arg, lookup, table};
use crate::queries;
use specforge_extension_sdk::prelude::{
    CommandBuilder, CommandCall, CommandError, ContributionsBuilder,
};
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
    c.command("milestone_timeline", |cmd| {
        cmd.title("Show the milestone timeline")
            .description(
                "Every milestone by target date, undated ones last, by id within \
            a date; a milestone not completed whose target date is before \
            --as-of (default today) is overdue.",
            )
            .category("query");
        as_of_arg(cmd);
        cmd.handler(|call| {
            let (date, as_of) = match as_of(call) {
                Ok(as_of) => as_of,
                Err(error) => return call.fail(&error, queries::INVALID_INPUT_EXIT),
            };
            let result = queries::milestone_timeline(call.graph(), as_of);
            call.render(&result, |out| {
                let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".to_string());
                let rows: Vec<Vec<String>> = result
                    .milestones
                    .iter()
                    .map(|m| {
                        vec![
                            m.milestone_id.clone(),
                            dash(&m.target_date),
                            dash(&m.status),
                            dash(&m.priority),
                            if m.is_overdue { "OVERDUE" } else { "" }.to_string(),
                        ]
                    })
                    .collect();
                out.push_str(&table(
                    &["milestone", "target_date", "status", "priority", "overdue"],
                    &rows,
                ));
                let _ = writeln!(out, "{} overdue as of {date}", result.overdue_count);
            })
        });
    });
    c.command("milestone_velocity", |cmd| {
        cmd.title("Show a milestone's velocity")
            .description(
                "A milestone's done, in-progress and remaining features, its \
            completion, the days elapsed since its start date (or target \
            date) as of --as-of (default today), features done per day and \
            the days that pace leaves.",
            )
            .category("query");
        entity_arg(cmd, "milestone");
        as_of_arg(cmd);
        cmd.handler(|call| {
            let (date, as_of) = match as_of(call) {
                Ok(as_of) => as_of,
                Err(error) => return call.fail(&error, queries::INVALID_INPUT_EXIT),
            };
            lookup(
                call,
                "milestone",
                |g, id| queries::milestone_velocity(g, id, as_of),
                |r, out| {
                    let or_dash = |v: Option<String>| v.unwrap_or_else(|| "-".to_string());
                    let _ = writeln!(out, "Milestone: {} (as of {date})", r.milestone_id);
                    let _ = writeln!(
                        out,
                        "Features: {} total, {} done, {} in progress, {} remaining",
                        r.total_features,
                        r.done_features,
                        r.in_progress_features,
                        r.remaining_features
                    );
                    let _ = writeln!(
                        out,
                        "Completion: {}",
                        or_dash(r.completion_ratio.map(|c| format!("{:.0}%", c * 100.0)))
                    );
                    let _ = writeln!(
                        out,
                        "Days elapsed: {}",
                        or_dash(r.days_elapsed.map(|d| d.to_string()))
                    );
                    let _ = writeln!(
                        out,
                        "Features per day: {}",
                        or_dash(r.features_per_day.map(|v| format!("{v:.2}")))
                    );
                    let _ = writeln!(
                        out,
                        "Days remaining: {}",
                        or_dash(r.days_remaining.map(|d| d.to_string()))
                    );
                },
            )
        });
    });
    c.command("weighted_milestone_completion", |cmd| {
        cmd.title("Show a milestone's effort-weighted completion")
            .description(
                "A milestone's completion weighted by effort (xs=1, s=2, m=3, \
            l=5, xl=8; a feature without one weighs as m), with its features \
            per effort level.",
            )
            .category("query");
        entity_arg(cmd, "milestone");
        cmd.handler(|call| {
            lookup(
                call,
                "milestone",
                queries::weighted_milestone_completion,
                |r, out| {
                    let _ = writeln!(out, "Milestone: {}", r.milestone_id);
                    match r.completion_ratio {
                        Some(ratio) => {
                            let _ = writeln!(
                                out,
                                "Weighted completion: {:.0}% ({}/{} effort points done)",
                                ratio * 100.0,
                                r.done_effort,
                                r.total_effort
                            );
                        }
                        None => {
                            let _ = writeln!(out, "Weighted completion: - (no features)");
                        }
                    }
                    let rows: Vec<Vec<String>> = r
                        .effort_breakdown
                        .iter()
                        .map(|e| {
                            let weight = queries::effort_weight(&e.effort_level);
                            vec![
                                e.effort_level.clone(),
                                weight.to_string(),
                                e.total.to_string(),
                                e.done.to_string(),
                            ]
                        })
                        .collect();
                    out.push_str(&table(&["effort", "weight", "features", "done"], &rows));
                },
            )
        });
    });
}

/// Declare `--as-of`, the date a command compares against ([`as_of`]).
fn as_of_arg(cmd: &mut CommandBuilder) {
    cmd.arg("as_of", |a| {
        a.description("The date to compare against, YYYY-MM-DD (default: today, UTC)");
    });
}

/// The date a command compares against, as written and in days
/// ([`queries::parse_ymd`]): `--as-of` when set, else the host's today.
/// `INVALID_INPUT` when it is not a `YYYY-MM-DD` date, or when neither is
/// set (a host that passes no date).
fn as_of(call: &CommandCall<'_>) -> Result<(String, i64), CommandError> {
    let (date, from) = match call.str("as_of") {
        Some(date) => (date, "as_of"),
        None if call.today().is_empty() => {
            return Err(queries::invalid_input(
                "no date to compare against: the host passed none; pass --as-of YYYY-MM-DD",
            ));
        }
        None => (call.today(), "the host's today"),
    };
    queries::parse_ymd(date)
        .map(|days| (date.to_string(), days))
        .ok_or_else(|| {
            queries::invalid_input(format!("{from} must be a date, YYYY-MM-DD, got '{date}'"))
        })
}
