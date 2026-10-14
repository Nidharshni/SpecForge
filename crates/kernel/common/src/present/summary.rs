//! The tally of a diagnostic list and its summary line.

use crate::{Diagnostic, Severity};
use serde::Serialize;
use std::collections::BTreeMap;

/// Error, warning and info counts of a diagnostic list: the one tally the
/// summary line, the exit code, `check`, stats and watch all read; stats'
/// `diagnostic_summary`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, crate::shape::Shape)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

impl Counts {
    /// The counts of `diagnostics`, by severity.
    pub fn of<'a>(diagnostics: impl IntoIterator<Item = &'a Diagnostic>) -> Self {
        let mut counts = Counts::default();
        for diagnostic in diagnostics {
            match diagnostic.severity {
                Severity::Error => counts.errors += 1,
                Severity::Warning => counts.warnings += 1,
                Severity::Info => counts.infos += 1,
            }
        }
        counts
    }
}

/// The summary line, wrapped in red when `color` is true and errors exist.
fn summary_line(diagnostics: &[Diagnostic], color: bool) -> String {
    let Counts {
        errors,
        warnings,
        infos,
    } = Counts::of(diagnostics);

    let plural = |n: usize, word: &str| -> String {
        if n == 1 {
            format!("{} {}", n, word)
        } else {
            format!("{} {}s", n, word)
        }
    };

    let text = format!(
        "{}, {}, {}",
        plural(errors, "error"),
        plural(warnings, "warning"),
        plural(infos, "info"),
    );

    if color && errors > 0 {
        format!("\x1b[1;31m{}\x1b[0m", text)
    } else {
        text
    }
}

/// The summary of `diagnostics`, grouped by code, showing the top
/// occurrences. Example:
/// ```text
/// 3 errors, 2 warnings, 0 infos
///   E001 (2)
///   E003 (1)
///   W001 (2)
/// ```
/// The first line is red only when `color` is true and errors exist;
/// otherwise the summary carries no ANSI escape. Up to five codes are
/// listed by frequency, then `... and N more from M other codes` and the
/// `specforge explain` hint. A clean list is the first line alone.
pub fn diagnostic_summary(diagnostics: &[Diagnostic], color: bool) -> String {
    let summary_line = summary_line(diagnostics, color);

    if diagnostics.is_empty() {
        return summary_line;
    }

    // Group by code, count occurrences
    let mut by_code: BTreeMap<&str, usize> = BTreeMap::new();
    for d in diagnostics {
        *by_code.entry(&d.code).or_insert(0) += 1;
    }

    // Sort by count descending, then code ascending
    let mut entries: Vec<(&&str, &usize)> = by_code.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

    // Show top codes (up to 5)
    let mut lines = vec![summary_line];
    for (code, count) in entries.iter().take(5) {
        lines.push(format!("  {} ({})", code, count));
    }
    if entries.len() > 5 {
        let remaining: usize = entries.iter().skip(5).map(|(_, c)| **c).sum();
        lines.push(format!(
            "  ... and {} more from {} other codes",
            remaining,
            entries.len() - 5
        ));
    }

    lines.push(String::from(
        "\nhint: run `specforge explain <code>` for details on any diagnostic code",
    ));

    lines.join("\n")
}
