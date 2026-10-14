use super::{entity_arg, ids, lookup};
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
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
}
