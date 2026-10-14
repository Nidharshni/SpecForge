use super::{entity_arg, ids, lookup, table};
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
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
}
