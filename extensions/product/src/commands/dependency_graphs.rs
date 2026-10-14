use super::{entity_arg, ids, lookup, page_args, paged, table};
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
    c.command("feature_ordering", |cmd| {
        cmd.title("Show features in dependency order")
            .description(
                "Every feature, dependencies before dependents, by priority \
            within a level (none counts as medium), then by id; features on \
            or behind a depends_on cycle come last, and the cycle's members \
            are reported.",
            )
            .category("query");

        cmd.handler(|call| {
            let result = queries::feature_ordering(call.graph());
            call.render(&result, |out| {
                if result.sorted_features.is_empty() {
                    let _ = writeln!(out, "No features.");
                    return;
                }
                for (n, id) in result.sorted_features.iter().enumerate() {
                    // `cycle_members` is sorted by id.
                    let flag = if result.cycle_members.binary_search(id).is_ok() {
                        "  (cycle)"
                    } else {
                        ""
                    };
                    let _ = writeln!(out, "{:>3}. {id}{flag}", n + 1);
                }
                if result.has_cycles {
                    let _ = writeln!(out, "Dependency cycle: {}", result.cycle_members.join(", "));
                }
            })
        });
    });
    c.command("critical_path", |cmd| {
        cmd.title("Show the critical path through milestones")
            .description(
                "The longest depends_on chain of milestones not yet completed, \
            earliest first, with target dates, zero slack and the blocked or \
            in-progress bottlenecks; empty, with a message, when milestones \
            depend on each other in a cycle.",
            )
            .category("query");

        cmd.handler(|call| {
            let result = queries::critical_path(call.graph());
            call.render(&result, |out| {
                if let Some(message) = &result.message {
                    let _ = writeln!(out, "No critical path: {message}");
                    return;
                }
                if result.critical_path.is_empty() {
                    let _ = writeln!(out, "No critical path: no milestone is still open.");
                    return;
                }
                let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".to_string());
                let rows: Vec<Vec<String>> = result
                    .critical_path
                    .iter()
                    .map(|n| {
                        vec![
                            n.entity_id.clone(),
                            dash(&n.target_date),
                            dash(&n.status),
                            n.slack_days.map_or("-".to_string(), |s| s.to_string()),
                        ]
                    })
                    .collect();
                out.push_str(&table(
                    &["milestone", "target_date", "status", "slack"],
                    &rows,
                ));
                if !result.bottleneck_ids.is_empty() {
                    let _ = writeln!(out, "Bottlenecks: {}", result.bottleneck_ids.join(", "));
                }
            })
        });
    });
    c.command("module_depth", |cmd| {
        cmd.title("Show a module's dependency depth")
            .description(
                "The longest depends_on chain from a module, the module first, \
            and its length in edges; -1 and the cycle's members when the \
            module is on or behind a dependency cycle.",
            )
            .category("query");
        entity_arg(cmd, "module");
        cmd.handler(|call| {
            lookup(
                call,
                "module",
                queries::module_dependency_depth,
                |r, out| {
                    if r.depth < 0 {
                        let _ = writeln!(
                            out,
                            "Module: {} (depth -1: on or behind a dependency cycle)",
                            r.module_id
                        );
                        let _ = writeln!(out, "Cycle: {}", r.longest_chain.join(", "));
                    } else {
                        let _ = writeln!(out, "Module: {} (depth {})", r.module_id, r.depth);
                        let _ = writeln!(out, "Chain: {}", r.longest_chain.join(" -> "));
                    }
                },
            )
        });
    });
    c.command("module_coupling", |cmd| {
        cmd.title("Show module coupling")
            .description(
                "Each module's fan-in, fan-out and their sum over depends_on, \
            most coupled first, paged, with the averages and the most \
            coupled module.",
            )
            .category("query");
        page_args(cmd);
        cmd.handler(|call| {
            let coupling = queries::module_coupling(call.graph());
            let extra = serde_json::json!({
                "avg_fan_in": coupling.avg_fan_in,
                "avg_fan_out": coupling.avg_fan_out,
                "most_coupled_id": coupling.most_coupled_id,
                "total_modules": coupling.total_modules,
            });
            paged(call, "modules", coupling.modules, extra, |page, out| {
                let rows: Vec<Vec<String>> = page
                    .items
                    .iter()
                    .map(|m| {
                        vec![
                            m.module_id.clone(),
                            m.fan_in.to_string(),
                            m.fan_out.to_string(),
                            m.coupling.to_string(),
                        ]
                    })
                    .collect();
                out.push_str(&table(&["module", "fan_in", "fan_out", "coupling"], &rows));
            })
        });
    });
    c.command("deliverable_dependents", |cmd| {
        cmd.title("Show deliverables that depend on a given deliverable")
            .description(
                "The deliverables that declare depends_on the deliverable, sorted \
            by id.",
            )
            .category("query");
        entity_arg(cmd, "deliverable");
        cmd.handler(|call| {
            lookup(
                call,
                "deliverable",
                queries::deliverable_dependents,
                |r, out| {
                    ids(
                        out,
                        &format!("Deliverables depending on '{}'", r.deliverable_id),
                        &r.dependents,
                    );
                },
            )
        });
    });
}
