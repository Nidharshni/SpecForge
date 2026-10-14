//! The `specforge product <command>` commands, each declared ([`declare`])
//! with the handler that answers its `cmd__product_<id>` export: the SDK
//! derives the `surfaces` payload and the export routing from the one
//! declaration. A handler reads its args through the declaration
//! ([`CommandCall`]), runs a query over the graph the host passes and
//! renders it in the format the host asked for (`human`, the CLI default,
//! or `json`, always over MCP; ADR 0011). The same exports serve the MCP
//! tools `specforge.product.<id>`.
//!
//! Under `json` a command prints one object, the payload type its surface
//! behavior names; under `human` its layout here, a table with a header row
//! where the payload is tabular. A command that cannot answer prints one
//! `ProductSurfaceError` to stderr and nothing to stdout: `ENTITY_NOT_FOUND`
//! (exit 1) with the nearest id of the kind, or `INVALID_INPUT` (exit 2).

use crate::queries::{self, ListEntry, ListFilter, ListKind};
use serde::Serialize;
use specforge_extension_sdk::prelude::{
    CommandBuilder, CommandCall, CommandError, CommandGraph, CommandOutput, ContributionsBuilder,
};
use std::fmt::Write as _;

/// Declare every command, each with the handler that answers it.
pub fn declare(c: &mut ContributionsBuilder) {
    c.command_prefix("product");
    list_command::<queries::FeatureListEntry>(
        c,
        "List features: each feature's id, title, status, priority, \
            problem and tags, filtered (every filter set must match), sorted \
            (by id unless --sort-by says otherwise) and paged, with the \
            total before paging.",
    );
    list_command::<queries::JourneyListEntry>(
        c,
        "List journeys: each journey's id, title, persona, channel and \
            feature counts, priority and tags, filtered (every filter set \
            must match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::DeliverableListEntry>(
        c,
        "List deliverables: each deliverable's id, title, artifact type, \
            status, journey and module counts and tags, filtered (every \
            filter set must match), sorted (by id unless --sort-by says \
            otherwise) and paged, with the total before paging.",
    );
    list_command::<queries::MilestoneListEntry>(
        c,
        "List milestones: each milestone's id, title, status, target \
            date, feature count, priority and tags, filtered (every filter \
            set must match), sorted (by id unless --sort-by says otherwise) \
            and paged, with the total before paging.",
    );
    list_command::<queries::ModuleListEntry>(
        c,
        "List modules: each module's id, title, family, feature count, \
            the modules it depends on and tags, filtered (every filter set \
            must match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::TermListEntry>(
        c,
        "List terms: each term's id, title, definition, alias count and \
            tags, filtered (every filter set must match), sorted (by id \
            unless --sort-by says otherwise) and paged, with the total \
            before paging.",
    );
    list_command::<queries::PersonaListEntry>(
        c,
        "List personas: each persona's id, title, technical level, \
            status, journey count and tags, filtered (every filter set must \
            match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::ChannelListEntry>(
        c,
        "List channels: each channel's id, title, interaction model, \
            status, journey count and tags, filtered (every filter set must \
            match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::ReleaseListEntry>(
        c,
        "List releases: each release's id, title, version, status, \
            deliverable count, release date and tags, filtered (every filter \
            set must match), sorted (by id unless --sort-by says otherwise) \
            and paged, with the total before paging.",
    );
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
    c.command("feature_impact", |cmd| {
        cmd.title("Show feature impact analysis")
            .description(
                "What deferring or removing a feature touches: the journeys, \
            milestones and modules that list it, the deliverables holding \
            those journeys or modules, the features depending on it \
            (transitively), and their total.",
            )
            .category("query");
        entity_arg(cmd, "feature");
        cmd.handler(|call| {
            lookup(call, "feature", queries::feature_impact, |r, out| {
                let _ = writeln!(
                    out,
                    "Feature: {} ({} affected)",
                    r.feature_id, r.total_affected_entities
                );
                let row = |what: &str, ids: &[String]| {
                    vec![
                        what.to_string(),
                        ids.len().to_string(),
                        if ids.is_empty() {
                            "-".to_string()
                        } else {
                            ids.join(", ")
                        },
                    ]
                };
                let rows = [
                    row("journeys", &r.affected_journeys),
                    row("milestones", &r.affected_milestones),
                    row("modules", &r.affected_modules),
                    row("deliverables", &r.affected_deliverables),
                    row("dependent features", &r.dependent_features),
                ];
                out.push_str(&table(&["affected", "count", "ids"], &rows));
            })
        });
    });
    c.command("feature_dependents", |cmd| {
        cmd.title("Show features that depend on a given feature")
            .description("The features that declare depends_on the feature, sorted by id.")
            .category("query");
        entity_arg(cmd, "feature");
        cmd.handler(|call| {
            lookup(call, "feature", queries::feature_dependents, |r, out| {
                ids(
                    out,
                    &format!("Features depending on '{}'", r.feature_id),
                    &r.dependents,
                );
            })
        });
    });
    c.command("persona_features", |cmd| {
        cmd.title("Show features reachable from a persona (via journeys)")
            .description(
                "The features of every journey the persona undertakes, and those \
            journeys, sorted.",
            )
            .category("query");
        entity_arg(cmd, "persona");
        cmd.handler(|call| {
            lookup(call, "persona", queries::persona_features, |r, out| {
                ids(
                    out,
                    &format!("Features for persona '{}'", r.persona_id),
                    &r.features,
                );
            })
        });
    });
    c.command("channel_features", |cmd| {
        cmd.title("Show features reachable from a channel (via journeys)")
            .description(
                "The features of every journey that uses the channel, and those \
            journeys, sorted.",
            )
            .category("query");
        entity_arg(cmd, "channel");
        cmd.handler(|call| {
            lookup(call, "channel", queries::channel_features, |r, out| {
                ids(
                    out,
                    &format!("Features for channel '{}'", r.channel_id),
                    &r.features,
                );
            })
        });
    });
    c.command("deliverable_traceability", |cmd| {
        cmd.title("Show the features a deliverable reaches")
            .description(
                "Every feature a deliverable reaches through its journeys or its \
            modules, once each, and how many each path reaches.",
            )
            .category("query");
        entity_arg(cmd, "deliverable");
        cmd.handler(|call| {
            lookup(
                call,
                "deliverable",
                queries::deliverable_traceability,
                |r, out| {
                    ids(
                        out,
                        &format!(
                            "Features of deliverable '{}' ({} via journeys, {} via modules)",
                            r.deliverable_id, r.journey_path_count, r.module_path_count
                        ),
                        &r.transitive_features,
                    );
                },
            )
        });
    });
    c.command("feature_deliverables", |cmd| {
        cmd.title("Show the deliverables holding a feature")
            .description(
                "Every deliverable that holds a feature through one of its \
            journeys or modules, once each, and how many each path reaches.",
            )
            .category("query");
        entity_arg(cmd, "feature");
        cmd.handler(|call| {
            lookup(call, "feature", queries::feature_deliverables, |r, out| {
                ids(
                    out,
                    &format!(
                        "Deliverables holding feature '{}' ({} via journeys, {} via \
            modules)",
                        r.feature_id, r.via_journey_count, r.via_module_count
                    ),
                    &r.deliverables,
                );
            })
        });
    });
    c.command("persona_channels", |cmd| {
        cmd.title("Show the channels a persona uses (via journeys)")
            .description(
                "The channels of every journey the persona undertakes, once each, \
            sorted.",
            )
            .category("query");
        entity_arg(cmd, "persona");
        cmd.handler(|call| {
            lookup(call, "persona", queries::persona_channels, |r, out| {
                ids(
                    out,
                    &format!("Channels of persona '{}'", r.persona_id),
                    &r.channels,
                );
            })
        });
    });
    c.command("deliverable_personas", |cmd| {
        cmd.title("Show the personas a deliverable serves (via journeys)")
            .description(
                "The personas the deliverable's journeys target, once each, \
            sorted, and the journeys that connect them.",
            )
            .category("query");
        entity_arg(cmd, "deliverable");
        cmd.handler(|call| {
            lookup(
                call,
                "deliverable",
                queries::deliverable_personas,
                |r, out| {
                    ids(
                        out,
                        &format!("Personas served by deliverable '{}'", r.deliverable_id),
                        &r.personas,
                    );
                    if !r.via_journey_ids.is_empty() {
                        let _ = writeln!(out, "Via journeys: {}", r.via_journey_ids.join(", "));
                    }
                },
            )
        });
    });
    c.command("deliverable_completion", |cmd| {
        cmd.title("Show deliverable completion")
            .description(
                "How many of the milestones a deliverable is tracked by are \
            completed, as a count and a ratio in [0, 1]; --details adds each \
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
    c.command("term_graph", |cmd| {
        cmd.title("Show the terms related to a term")
            .description(
                "The terms reachable from a term over see_also within --max-hops \
            (default 1, at most 5), sorted by id, the term itself left out.",
            )
            .category("query");
        entity_arg(cmd, "term");
        cmd.arg("max_hops", |a| {
            a.count().description(
                "How many see_also hops to follow (default 1; above 5 counts as \
            5)",
            );
        });
        cmd.handler(|call| {
            let max_hops = call.count("max_hops");
            lookup(
                call,
                "term",
                |g, id| queries::term_graph(g, id, max_hops),
                |r, out| {
                    ids(
                        out,
                        &format!(
                            "Terms related to '{}' within {} see_also hop{}",
                            r.term_id,
                            r.max_hops,
                            if r.max_hops == 1 { "" } else { "s" }
                        ),
                        &r.related_terms,
                    );
                },
            )
        });
    });
    c.command("term_clusters", |cmd| {
        cmd.title("Show clusters of related terms")
            .description(
                "The connected components of the see_also graph between terms \
            (undirected), largest first, ties by first term id; terms with \
            no see_also link are counted as isolated.",
            )
            .category("query");

        cmd.handler(|call| {
            let result = queries::term_clusters(call.graph());
            call.render(&result, |out| {
                let rows: Vec<Vec<String>> = result
                    .clusters
                    .iter()
                    .map(|c| {
                        vec![
                            c.cluster_id.to_string(),
                            c.term_count.to_string(),
                            c.term_ids.join(", "),
                        ]
                    })
                    .collect();
                out.push_str(&table(&["cluster", "terms", "ids"], &rows));
                let _ = writeln!(
                    out,
                    "{} clusters, {} isolated of {} terms",
                    result.cluster_count, result.isolated_count, result.total_terms
                );
            })
        });
    });
    c.command("term_density", |cmd| {
        cmd.title("Show how connected the glossary is")
            .description(
                "Terms, see_also references between them, average and maximum \
            connections, hub terms (more than twice the average and at least \
            3) and isolated terms.",
            )
            .category("query");

        cmd.handler(|call| {
            let result = queries::term_density(call.graph());
            call.render(&result, |out| {
                let list = |ids: &[String]| {
                    if ids.is_empty() {
                        "-".to_string()
                    } else {
                        ids.join(", ")
                    }
                };
                let _ = writeln!(out, "Terms:           {}", result.total_terms);
                let _ = writeln!(out, "see_also edges:  {}", result.total_see_also);
                let _ = writeln!(
                    out,
                    "Avg connections: {}",
                    result
                        .avg_connections
                        .map_or("-".to_string(), |a| format!("{a:.2}"))
                );
                let _ = writeln!(out, "Max connections: {}", result.max_connections);
                let _ = writeln!(
                    out,
                    "Hubs ({}):        {}",
                    result.hub_terms.len(),
                    list(&result.hub_terms)
                );
                let _ = writeln!(
                    out,
                    "Isolated ({}):    {}",
                    result.isolated_terms.len(),
                    list(&result.isolated_terms)
                );
            })
        });
    });
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
    c.command("bulk_status", |cmd| {
        cmd.title("Show status breakdown across all entity kinds")
            .description(
                "For each product kind with a lifecycle, how many entities have \
            each status.",
            )
            .category("query");

        cmd.handler(|call| {
            let result = queries::bulk_status(call.graph());
            call.render(&result, |out| {
                if result.kinds.is_empty() {
                    let _ = writeln!(out, "No status-bearing entities found.");
                    return;
                }
                let rows: Vec<Vec<String>> = result
                    .kinds
                    .iter()
                    .flat_map(|k| {
                        k.by_status
                            .iter()
                            .map(|s| vec![k.kind.clone(), s.status.clone(), s.count.to_string()])
                    })
                    .collect();
                out.push_str(&table(&["kind", "status", "count"], &rows));
            })
        });
    });
    c.command("health", |cmd| {
        cmd.title("Show project health score")
            .description(
                "A 0-100 score averaging coverage, connectivity and completeness \
            of the product graph, with entity and unreferenced counts.",
            )
            .category("query");

        cmd.handler(|call| {
            let report = queries::project_health(call.graph());
            call.render(&report, |out| health(&report, out))
        });
    });
}

/// The list command over `T`'s kind, `specforge product <plural>`: its
/// filter args are the kind's [`ListKind::filters`], in order, each with its
/// help in `filter_help`, then the tags, sort and page args every list takes.
fn list_command<T: ListEntry + 'static>(c: &mut ContributionsBuilder, description: &str) {
    let kind = T::KIND;
    c.command(kind.plural, |cmd| {
        cmd.title(&format!("List {}", kind.plural))
            .description(description)
            .category("query");
        // A closed filter is `one_of` its field's vocabulary (the one the
        // field is declared an enum of): the SDK refuses any other value, and
        // the CLI help and the MCP tool's schema list them.
        for filter in kind.filters {
            cmd.arg(filter.field(), |a| {
                if let Some(vocabulary) = kind.vocabulary(filter.field()) {
                    a.one_of(vocabulary.values());
                }
                a.description(&filter.help(kind));
            });
        }
        cmd.arg("tags", |a| {
            a.description(&format!(
                "Only {} with at least one of these tags (comma-separated)",
                kind.plural
            ));
        })
        .arg("sort_by", |a| {
            a.description(&format!(
                "The field to sort by (default id): id, title or a field a {} declares; ties by id",
                kind.kind
            ));
        })
        .arg("sort_order", |a| {
            a.one_of(queries::SORT_ORDER)
                .description("asc (the default) or desc");
        });
        page_args(cmd);
        cmd.handler(list::<T>);
    });
}

/// A list command over `T`'s kind: the page its filter args select, its
/// entries under the kind's plural. A sort field the kind does not have is
/// `INVALID_INPUT` (the SDK refuses a value outside a closed filter's enum
/// or `asc`/`desc`, and a page arg that is not a count).
fn list<T: ListEntry>(call: &CommandCall<'_>) -> CommandOutput {
    let filter = match list_filter(call, T::KIND) {
        Ok(filter) => filter,
        Err(error) => return call.fail(&error, queries::INVALID_INPUT_EXIT),
    };
    let page = queries::list::<T>(call.graph(), &filter);
    call.render(&page.payload(T::KIND.plural), |out| {
        let rows: Vec<Vec<String>> = page.items.iter().map(ListEntry::row).collect();
        out.push_str(&table(T::HEADERS, &rows));
        if page.has_more {
            let _ = writeln!(
                out,
                "{} of {} {}; --offset {} for more",
                page.items.len(),
                page.total,
                T::KIND.plural,
                page.offset + page.items.len()
            );
        }
    })
}

/// The `ListFilter` `call`'s args set for `kind`, validated.
fn list_filter<'a>(
    call: &'a CommandCall<'_>,
    kind: &'a ListKind,
) -> Result<ListFilter<'a>, CommandError> {
    let mut filter = ListFilter::all(kind);
    (filter.offset, filter.limit) = page(call);
    for arg in kind.filters {
        if let Some(value) = call.str(arg.field()) {
            filter.equals.push((arg.field(), value));
        }
    }
    if let Some(tags) = call.str("tags") {
        filter.tags = tags
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
    }
    if let Some(field) = call.str("sort_by") {
        if !queries::sortable(kind.kind, field) {
            return Err(queries::invalid_input(format!(
                "sort_by: a {} has no field '{field}'",
                kind.kind
            )));
        }
        filter.sort_by = field;
    }
    if let Some(order) = call.str("sort_order") {
        filter.descending = order == "desc";
    }
    Ok(filter)
}

/// Declare the positional arg naming the entity of `kind` a query is about.
fn entity_arg(cmd: &mut CommandBuilder, kind: &str) {
    cmd.arg(kind, |a| {
        a.required().description(&format!("The {kind} id"));
    });
}

/// Declare the `--limit` and `--offset` counts of a paged command.
fn page_args(cmd: &mut CommandBuilder) {
    cmd.arg("limit", |a| {
        a.count()
            .description("Return at most this many (default 100, clamped to 1-1000)");
    })
    .arg("offset", |a| {
        a.count().description("Skip this many first (default 0)");
    });
}

/// The `--offset` and `--limit` `call` sets ([`page_args`]).
fn page(call: &CommandCall<'_>) -> (Option<usize>, Option<usize>) {
    (call.count("offset"), call.count("limit"))
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

/// A paged project-wide command: one page of `items` under `key` beside
/// the fields `extra` adds.
fn paged<T: Serialize>(
    call: &CommandCall<'_>,
    key: &str,
    items: Vec<T>,
    extra: serde_json::Value,
    human: impl FnOnce(&queries::Page<T>, &mut String),
) -> CommandOutput {
    let (offset, limit) = page(call);
    let page = queries::paginate(items, offset, limit);
    let mut payload = page.payload(key);
    if let (Some(payload), serde_json::Value::Object(extra)) = (payload.as_object_mut(), extra) {
        payload.extend(extra);
    }
    call.render(&payload, |out| {
        human(&page, out);
        if page.has_more {
            let _ = writeln!(
                out,
                "{} of {} {key}; --offset {} for more",
                page.items.len(),
                page.total,
                page.offset + page.items.len()
            );
        }
    })
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

/// A query about the entity the positional arg `kind` names
/// ([`entity_arg`]): its payload rendered, or `ENTITY_NOT_FOUND` (exit 1)
/// with the nearest id of the kind.
fn lookup<T: Serialize>(
    call: &CommandCall<'_>,
    kind: &str,
    query: impl FnOnce(&CommandGraph, &str) -> Option<T>,
    human: impl FnOnce(&T, &mut String),
) -> CommandOutput {
    let id = call.str(kind).unwrap_or_default();
    match query(call.graph(), id) {
        Some(result) => call.render(&result, |out| human(&result, out)),
        None => call.fail(
            &queries::not_found(call.graph(), kind, id),
            queries::NOT_FOUND_EXIT,
        ),
    }
}

/// Entity ids, one per line under `heading`.
fn ids(out: &mut String, heading: &str, ids: &[String]) {
    let _ = writeln!(out, "{heading}:");
    for i in ids {
        let _ = writeln!(out, "  {i}");
    }
    if ids.is_empty() {
        let _ = writeln!(out, "  (none)");
    }
}

fn health(report: &queries::HealthPayload, out: &mut String) {
    let _ = writeln!(out, "Project Health Score: {:.0}/100", report.score.overall);
    let _ = writeln!(out, "  Coverage:     {:.0}%", report.score.coverage);
    let _ = writeln!(out, "  Connectivity: {:.0}%", report.score.connectivity);
    let _ = writeln!(out, "  Completeness: {:.0}%", report.score.completeness);
    let _ = writeln!(out);
    let _ = writeln!(out, "Entity counts:");
    for ec in report.entity_counts.iter().filter(|ec| ec.count > 0) {
        let _ = writeln!(out, "  {}: {}", ec.kind, ec.count);
    }
    if !report.unreferenced_counts.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Unreferenced entities:");
        for oc in report
            .unreferenced_counts
            .iter()
            .filter(|oc| oc.unreferenced > 0)
        {
            let _ = writeln!(out, "  {}: {}/{}", oc.kind, oc.unreferenced, oc.total);
        }
    }
}

/// `rows` under `headers`, each column as wide as its widest cell, two
/// spaces apart; no trailing spaces.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let header: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    for row in std::iter::once(&header).chain(rows) {
        let mut line = String::new();
        for (i, (cell, width)) in row.iter().zip(&widths).enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            let _ = write!(line, "{cell:<width$}");
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
