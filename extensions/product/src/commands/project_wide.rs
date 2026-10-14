use super::table;
use crate::queries;
use specforge_extension_sdk::prelude::ContributionsBuilder;
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
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
