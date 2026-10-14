use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_ops::read::stats::Stats;
use specforge_ops::view::ProjectView;

// `specforge stats`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    /// `specforge stats`: the stats operation over the project compiled at
    /// `path`, its proof percentage read from what `specforge collect` last
    /// recorded there; a report that is there but unusable is an error
    /// (exit 2), as in `analyze`.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let format = self.output.format;
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::measuring(self.output.format).report(terminal, &error),
        };
        let stats = match specforge_ops::read::stats::stats(&ProjectView::of(&project)) {
            Ok(stats) => stats,
            Err(error) => return Refusal::measuring(format).report(terminal, &error),
        };

        match format {
            OutputFormat::Json => terminal.json(&stats),
            OutputFormat::Human => print_human(&stats, terminal),
        }

        Exit::Passed
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;

    #[test]
    fn stats_writes_its_human_report_to_stdout() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main.spec"),
            "behavior parse_input \"Parse\" {\n  contract \"x\"\n}\n",
        )
        .unwrap();
        let (terminal, capture) = Terminal::captured("", Facts::PIPED);

        let exit = Args {
            project: crate::project::ProjectArg::at(dir.path()),
            output: OutputArg {
                format: OutputFormat::Human,
            },
        }
        .run(&terminal);

        assert_eq!(exit, Exit::Passed);
        assert!(
            capture.stdout().starts_with("Entities: 1\n  behavior: 1\n"),
            "{}",
            capture.stdout()
        );
        assert_eq!(capture.stderr(), "");
    }
}
