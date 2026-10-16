//! The collectors runner extensions declare: what selects each, the command it runs, where its report
//! lands.

use crate::{Containment, Held, OpError, OpErrorKind};
use specforge_common::codes;
use specforge_protocol_types::ExtensionDeclaration;
use std::path::Path;

/// A collector contributed by an enabled extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collector {
    pub extension: String,
    pub name: String,
    pub export: String,
    pub detect: Vec<String>,
    pub run: Vec<String>,
    /// Report file or directory, relative to the project root.
    pub report: String,
    /// What else the host keeps from the run: `"stdout"` or nothing.
    pub capture: Option<String>,
}

/// The only `capture` value: the command's standard output.
pub(super) const CAPTURE_STDOUT: &str = "stdout";

/// Every collector the enabled extensions declare, in declaration order.
pub fn collectors(declarations: &[ExtensionDeclaration]) -> Vec<Collector> {
    declarations
        .iter()
        .flat_map(|d| {
            d.collectors.iter().map(|c| Collector {
                extension: d.name().to_string(),
                name: c.name.clone(),
                export: c.export.clone(),
                detect: c
                    .auto_detect
                    .as_ref()
                    .map(|d| d.file_patterns.clone())
                    .unwrap_or_default(),
                run: c.run.clone(),
                report: c
                    .report
                    .clone()
                    .unwrap_or_else(|| format!(".specforge/reports/{}.json", c.name)),
                capture: c.capture.clone(),
            })
        })
        .collect()
}

/// Collectors whose detection files are present at the project root.
pub(super) fn detect<'a>(collectors: &'a [Collector], root: &Path) -> Vec<&'a Collector> {
    collectors
        .iter()
        .filter(|c| c.detect.iter().any(|p| pattern_matches(root, p)))
        .collect()
}

/// A detection pattern is a path relative to the project root whose last
/// segment may use `*` wildcards (`vitest.config.*`).
fn pattern_matches(root: &Path, pattern: &str) -> bool {
    let path = Path::new(pattern);
    let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };
    if !file.contains('*') {
        return root.join(path).exists();
    }
    let dir = root.join(path.parent().unwrap_or(Path::new("")));
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_str().is_some_and(|n| wildcard(file, n)))
}

pub(super) fn wildcard(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name.ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// The report location, held. An extension may only declare a relative name without `..`
/// ([`crate::containment::declared`], E058); where the name lands is the request's containment
/// (a symlink that leaves the project is `outside_root`).
pub(super) fn report_path(
    collector: &Collector,
    root: &Path,
    containment: &Containment,
) -> Result<Held, OpError> {
    let Some(relative) = crate::containment::declared(Path::new(&collector.report)) else {
        return Err(OpError::coded(
            OpErrorKind::PreconditionFailed,
            codes::E058,
            format!(
                "collector '{}' of {} declares report path '{}' outside the project",
                collector.name, collector.extension, collector.report
            ),
        ));
    };
    Ok(containment.resolve(&root.join(relative))?)
}

/// Whether the collector keeps its command's standard output. Any value
/// other than `"stdout"` is refused.
pub(super) fn captures_stdout(collector: &Collector) -> Result<bool, String> {
    match collector.capture.as_deref() {
        None => Ok(false),
        Some(CAPTURE_STDOUT) => Ok(true),
        Some(other) => Err(format!(
            "collector '{}' of {} declares unknown capture '{other}' (only \"stdout\")",
            collector.name, collector.extension
        )),
    }
}

/// Where a captured standard output is kept, held: inside a report directory
/// (a report path without an extension), else next to the report file.
pub(super) fn capture_path(
    collector: &Collector,
    report: &Held,
    containment: &Containment,
) -> Result<Held, OpError> {
    let spelled = &report.spelled;
    let named = if report.real.is_dir() || report.real.extension().is_none() {
        spelled.join(format!("{}.stdout.txt", collector.name))
    } else {
        let mut name = spelled.file_name().unwrap_or_default().to_os_string();
        name.push(".stdout.txt");
        spelled.with_file_name(name)
    };
    Ok(containment.resolve(&named)?)
}

/// The captured standard output at `held`, if there is one. Runner output
/// isn't always UTF-8 (tests print what they like), so it's read lossily.
pub(super) fn read_capture(held: &Held) -> Option<String> {
    held.read()
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The declared command with `{report}` expanded.
pub(super) fn command_line(collector: &Collector, report: &Path) -> Vec<String> {
    let report = report.display().to_string();
    collector
        .run
        .iter()
        .map(|arg| arg.replace("{report}", &report))
        .collect()
}
