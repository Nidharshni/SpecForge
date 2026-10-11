use specforge_ops::stats::Stats;
use specforge_ops::view::ProjectView;
use std::path::Path;

use crate::OutputFormat;
use crate::outcome::{Exit, Refusal, print_json};
use crate::pipeline;

/// `specforge stats`: the stats operation over the project compiled at
/// `path`, its proof percentage read from what `specforge collect` last
/// recorded there; a report that is there but unusable is an error (exit
/// 2), as in `analyze`.
pub fn run(path: &Path, format: OutputFormat) -> Exit {
    let project = pipeline::compile_project(path);
    let stats = match specforge_ops::stats::stats(&ProjectView::of(&project)) {
        Ok(stats) => stats,
        Err(error) => return Refusal::measuring(format).report(&error),
    };

    match format {
        OutputFormat::Json => print_json(&stats),
        OutputFormat::Human => print_human(&stats),
    }

    Exit::Passed
}

fn print_human(stats: &Stats) {
    println!("Entities: {}", stats.entity_count);
    for row in &stats.entity_counts {
        println!("  {}: {}", row.kind, row.count);
    }
    println!("Edges:    {}", stats.edge_count);
    println!("Unconnected: {}", stats.unconnected_count);
    println!("Verified: {}", stats.verified_count);
    println!(
        "Declared: {}% of {} testable",
        stats.declared_pct.round(),
        stats.testable_count
    );
    if let Some(proof_pct) = stats.proof_pct {
        println!(
            "Proven:   {}% of {} testable",
            proof_pct.round(),
            stats.testable_count
        );
    }
    let found = stats.diagnostic_summary;
    if found.errors > 0 || found.warnings > 0 || found.infos > 0 {
        println!(
            "Diagnostics: {} errors, {} warnings, {} info",
            found.errors, found.warnings, found.infos
        );
    }
}
