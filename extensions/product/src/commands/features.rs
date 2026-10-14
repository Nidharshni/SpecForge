use super::{entity_arg, ids, lookup, table};
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
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
}
