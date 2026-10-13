//! `specforge infer-status`: the shared inference progress and gaps
//! (`specforge_ops::infer`), presented. The JSON document is the MCP
//! `specforge.infer_progress` answer; `--gaps` adds the unanalyzed files
//! by directory and `--gaps-detail` the `specforge.infer_gaps` report.

use crate::terminal::{Terminal, out, outln};

use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use serde::Serialize;
use specforge_ops::infer::{
    self, Gaps, GapsDocument, MANIFEST_FILENAME, Progress, ProgressDocument, UnanalyzedDirectory,
};
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;

/// What `specforge infer-status --format json` prints: the progress document
/// `specforge.infer_progress` answers, and the sections only the CLI's flags
/// ask for.
#[derive(Serialize)]
struct Printed {
    #[serde(flatten)]
    progress: ProgressDocument,
    /// `--gaps`.
    #[serde(skip_serializing_if = "Option::is_none")]
    unanalyzed_by_directory: Option<Vec<UnanalyzedDirectory>>,
    /// `--gaps-detail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    gap_analysis: Option<GapsDocument>,
}

// `specforge infer-status`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,

    /// Show unanalyzed files grouped by directory
    #[arg(long)]
    gaps: bool,

    /// Show stale and deleted files
    #[arg(long)]
    stale: bool,

    /// Show detailed gap analysis (pub Rust items without spec entities)
    #[arg(long)]
    gaps_detail: bool,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        let (format, show_gaps, show_stale, show_gaps_detail) =
            (self.output.format, self.gaps, self.stale, self.gaps_detail);
        let project = match self.project.compile() {
            Ok(project) => project,
            Err(error) => return Refusal::of(format).report(terminal, &error),
        };
        run_over(
            &project,
            format,
            show_gaps,
            show_stale,
            show_gaps_detail,
            terminal,
        )
    }
}

fn run_over(
    project: &CompiledProject,
    format: OutputFormat,
    show_gaps: bool,
    show_stale: bool,
    show_gaps_detail: bool,
    terminal: &Terminal,
) -> Exit {
    let view = ProjectView::of(project);
    let progress = match infer::progress(&view) {
        Ok(progress) => progress,
        Err(error) => {
            return Refusal::of(format).report(terminal, &error);
        }
    };
    let gaps = if show_gaps_detail {
        match infer::gaps(&view) {
            Ok(gaps) => Some(gaps),
            Err(error) => {
                return Refusal::of(format).report(terminal, &error);
            }
        }
    } else {
        None
    };

    match format {
        OutputFormat::Json => {
            terminal.json(&Printed {
                progress: progress.document(),
                unanalyzed_by_directory: show_gaps.then(|| progress.unanalyzed_by_directory()),
                gap_analysis: gaps.as_ref().map(Gaps::document),
            });
        }
        OutputFormat::Human => {
            out!(
                terminal,
                "{}",
                render_human(&progress, show_gaps, show_stale, gaps.as_ref())
            );
            if !progress.recorded {
                outln!(terminal);
                outln!(
                    terminal,
                    "No {MANIFEST_FILENAME} yet: start an inference with the MCP infer prompt \
                     (specforge://prompts/infer), which records progress here."
                );
            }
        }
    }
    Exit::Passed
}

fn render_human(progress: &Progress, gaps: bool, stale: bool, detail: Option<&Gaps>) -> String {
    let mut out = String::new();
    macro_rules! line {
        () => { out.push('\n') };
        ($($arg:tt)*) => {{ out.push_str(&format!($($arg)*)); out.push('\n'); }};
    }
    let summary = &progress.summary;
    let pct = if summary.files_total > 0 {
        (summary.files_analyzed as f64 / summary.files_total as f64) * 100.0
    } else {
        0.0
    };
    line!("Inference Progress");
    line!(
        "  Files:    {}/{} ({:.0}%)",
        summary.files_analyzed,
        summary.files_total,
        pct
    );
    line!("  Entities: {}", summary.entities_produced);
    if !progress.stale.is_empty() || !progress.deleted.is_empty() {
        line!("  Stale:    {}", progress.stale.len());
        line!("  Deleted:  {}", progress.deleted.len());
    }

    if !progress.sessions.is_empty() {
        line!();
        line!("Sessions:");
        for session in &progress.sessions {
            line!(
                "  {}  {}  {}  {} \u{2192} {}",
                session.session_id,
                session.agent,
                session.status.name(),
                session.started_at,
                session.ended_at.as_deref().unwrap_or("\u{2026}")
            );
        }
    }

    if gaps && !progress.unanalyzed.is_empty() {
        line!();
        line!("Unanalyzed files:");
        for row in progress.unanalyzed_by_directory() {
            line!("  {} ({} files)", row.directory, row.count);
            for f in &row.files {
                line!("    {f}");
            }
        }
    }
    if stale && !progress.stale.is_empty() {
        line!();
        line!("Stale files (content changed since analysis):");
        for f in &progress.stale {
            line!("  {f}");
        }
    }
    if stale && !progress.deleted.is_empty() {
        line!();
        line!("Deleted files (in manifest but missing from disk):");
        for f in &progress.deleted {
            line!("  {f}");
        }
    }

    if let Some(report) = detail {
        let scanners = if report.scanners_used.is_empty() {
            "no scanners".to_string()
        } else {
            report.scanners_used.join(", ")
        };
        line!();
        line!("Gap Analysis (via {scanners})");
        line!("  Public items: {}", report.total_pub_items);
        line!("  Covered:      {}", report.covered_items);
        line!("  Gaps:         {}", report.gap_count());
        if !report.scan_failures.is_empty() {
            line!();
            line!(
                "  Approximate: a scanner failed on {} file(s):",
                report.scan_failures.len()
            );
            for failure in &report.scan_failures {
                let diagnostic = failure.error.diagnostic();
                line!(
                    "    {}: error[{}]: {}",
                    failure.file,
                    diagnostic.code,
                    diagnostic.message
                );
            }
        }
        if report.gap_count() > 0 {
            line!();
            for (dir, items) in &report.by_directory {
                line!("  {} ({} uncovered)", dir, items.len());
                for g in items {
                    line!("    {}:{} {} ({})", g.file, g.line, g.name, g.item_kind);
                }
            }
        }
    }
    out
}
