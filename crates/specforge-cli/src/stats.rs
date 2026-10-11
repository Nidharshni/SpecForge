use crate::terminal::{Terminal, outln};
use specforge_ops::stats::Stats;
use specforge_ops::view::ProjectView;
use std::path::Path;

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::pipeline;

/// `specforge stats`: the stats operation over the project compiled at
/// `path`, its proof percentage read from what `specforge collect` last
/// recorded there; a report that is there but unusable is an error (exit
/// 2), as in `analyze`.
pub fn run(path: &Path, format: OutputFormat, terminal: &Terminal) -> Exit {
    let project = pipeline::compile_project(path);
    let stats = match specforge_ops::stats::stats(&ProjectView::of(&project)) {
        Ok(stats) => stats,
        Err(error) => return Refusal::measuring(format).report(terminal, &error),
    };

    match format {
        OutputFormat::Json => terminal.json(&stats),
        OutputFormat::Human => print_human(&stats, terminal),
    }

    Exit::Passed
}

fn print_human(stats: &Stats, terminal: &Terminal) {
    outln!(terminal, "Entities: {}", stats.entity_count);
    for row in &stats.entity_counts {
        outln!(terminal, "  {}: {}", row.kind, row.count);
    }
    outln!(terminal, "Edges:    {}", stats.edge_count);
    outln!(terminal, "Unconnected: {}", stats.unconnected_count);
    outln!(terminal, "Verified: {}", stats.verified_count);
    outln!(
        terminal,
        "Declared: {}% of {} testable",
        stats.declared_pct.round(),
        stats.testable_count
    );
    if let Some(proof_pct) = stats.proof_pct {
        outln!(
            terminal,
            "Proven:   {}% of {} testable",
            proof_pct.round(),
            stats.testable_count
        );
    }
    let found = stats.diagnostic_summary;
    if found.errors > 0 || found.warnings > 0 || found.infos > 0 {
        outln!(
            terminal,
            "Diagnostics: {} errors, {} warnings, {} info",
            found.errors,
            found.warnings,
            found.infos
        );
    }
}
