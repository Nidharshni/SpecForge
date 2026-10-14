//! Inference gaps: the public items of the project's source files that no
//! graph entity names (`specforge infer-status --gaps-detail`, the MCP
//! `specforge.infer_gaps` tool).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use specforge_common::shape::Shape;

use super::ScanFailure;
use super::discovery::source_files;
use super::manifest::InferenceManifest;
use crate::OpError;
use crate::view::ProjectView;

/// Public source items no graph entity names, by directory.
#[derive(Debug, Clone)]
pub struct Gaps {
    pub total_pub_items: usize,
    pub covered_items: usize,
    /// Directory (`.` for the root) to its uncovered items, sorted.
    pub by_directory: BTreeMap<String, Vec<SourceItem>>,
    /// Whether the counts are approximate: a regex fallback found the
    /// items (no scanner extension), or a scanner failed on a file.
    pub approximate: bool,
    pub scanners_used: Vec<String>,
    /// The files a scanner failed on (E028 each): their items are unknown.
    pub scan_failures: Vec<ScanFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceItem {
    pub name: String,
    pub item_kind: String,
    pub file: String,
    pub line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapReport {
    pub total_pub_items: usize,
    pub covered_items: usize,
    pub gaps: Vec<SourceItem>,
    pub approximate: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scanners_used: Vec<String>,
}

pub fn compute_gap_report(
    scanned_items: Vec<SourceItem>,
    entity_ids: &[&str],
    scanners_used: Vec<String>,
) -> GapReport {
    let total = scanned_items.len();
    let entity_set: std::collections::HashSet<&str> = entity_ids.iter().copied().collect();

    let gaps: Vec<SourceItem> = scanned_items
        .into_iter()
        .filter(|item| !entity_set.contains(item.name.as_str()))
        .collect();

    GapReport {
        total_pub_items: total,
        covered_items: total - gaps.len(),
        scanners_used,
        gaps,
        approximate: false,
    }
}

/// The public items of the view's source files that no entity of its
/// graph names, scanned through its extensions' scanners in the view's
/// runtime. Without a root: `no_project`.
pub fn gaps(view: &ProjectView) -> Result<Gaps, OpError> {
    let root = view.project_root()?;
    let declarations = view.registries().declarations();
    let files = source_files(root, declarations, &InferenceManifest::at(root)?);
    let scanned = crate::scan::scan_source_files(
        view.runtime().map(|r| r.as_ref()),
        declarations,
        root,
        &files,
    );
    let entity_ids: Vec<&str> = view
        .graph()
        .nodes()
        .into_iter()
        .map(|n| n.id.raw.as_str())
        .collect();
    let report = compute_gap_report(scanned.items, &entity_ids, scanned.scanners_used);
    let mut by_directory: BTreeMap<String, Vec<SourceItem>> = BTreeMap::new();
    for gap in report.gaps {
        by_directory
            .entry(directory_of(&gap.file).to_string())
            .or_default()
            .push(gap);
    }
    Ok(Gaps {
        total_pub_items: report.total_pub_items,
        covered_items: report.covered_items,
        by_directory,
        approximate: report.approximate || !scanned.failures.is_empty(),
        scanners_used: report.scanners_used,
        scan_failures: scanned.failures,
    })
}

/// The directory part of a project-relative path, `.` at the root.
pub fn directory_of(path: &str) -> &str {
    path.rfind('/').map_or(".", |i| &path[..i])
}

impl Gaps {
    /// How many items no entity names.
    pub fn gap_count(&self) -> usize {
        self.by_directory.values().map(Vec::len).sum()
    }

    /// The document both surfaces answer with (the spec's
    /// `InferenceGapReport`): totals, then each directory's items.
    pub fn document(&self) -> GapsDocument {
        GapsDocument {
            total_pub_items: self.total_pub_items,
            covered_items: self.covered_items,
            gap_count: self.gap_count(),
            approximate: self.approximate,
            scanners_used: self.scanners_used.clone(),
            scan_failures: self
                .scan_failures
                .iter()
                .map(|failure| {
                    let diagnostic = failure.error.diagnostic();
                    ScanFailureRow {
                        file: failure.file.clone(),
                        code: diagnostic.code,
                        message: diagnostic.message,
                    }
                })
                .collect(),
            by_directory: self
                .by_directory
                .iter()
                .map(|(dir, gaps)| DirectoryGaps {
                    directory: dir.clone(),
                    count: gaps.len(),
                    items: gaps
                        .iter()
                        .map(|g| GapItem {
                            name: g.name.clone(),
                            item_kind: g.item_kind.clone(),
                            file: g.file.clone(),
                            line: g.line,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// What inference gaps answer with, on both surfaces (the spec's
/// `InferenceGapReport`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct GapsDocument {
    pub total_pub_items: usize,
    pub covered_items: usize,
    pub gap_count: usize,
    pub approximate: bool,
    pub scanners_used: Vec<String>,
    pub scan_failures: Vec<ScanFailureRow>,
    pub by_directory: Vec<DirectoryGaps>,
}

/// A file the scan could not read.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ScanFailureRow {
    pub file: String,
    pub code: String,
    pub message: String,
}

/// The gaps of one directory.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct DirectoryGaps {
    pub directory: String,
    pub count: usize,
    pub items: Vec<GapItem>,
}

/// One public item no entity names.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct GapItem {
    pub name: String,
    pub item_kind: String,
    pub file: String,
    pub line: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_report_identifies_uncovered_items() {
        let items = vec![
            SourceItem {
                name: "hello".into(),
                item_kind: "function".into(),
                file: "src/lib.rs".into(),
                line: 1,
                scanner: Some("rust".into()),
            },
            SourceItem {
                name: "config".into(),
                item_kind: "struct".into(),
                file: "src/lib.rs".into(),
                line: 2,
                scanner: Some("rust".into()),
            },
        ];
        let report = compute_gap_report(items, &["hello"], vec!["rust".into()]);
        assert_eq!(report.total_pub_items, 2);
        assert_eq!(report.covered_items, 1);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].name, "config");
        assert!(!report.approximate);
        assert_eq!(report.scanners_used, vec!["rust"]);
    }

    #[test]
    fn gap_report_all_covered() {
        let items = vec![SourceItem {
            name: "hello".into(),
            item_kind: "function".into(),
            file: "src/lib.rs".into(),
            line: 1,
            scanner: Some("rust".into()),
        }];
        let report = compute_gap_report(items, &["hello"], vec!["rust".into()]);
        assert_eq!(report.gaps.len(), 0);
        assert_eq!(report.covered_items, 1);
    }

    #[test]
    fn gap_report_empty_items() {
        let report = compute_gap_report(vec![], &["hello"], vec![]);
        assert_eq!(report.total_pub_items, 0);
        assert_eq!(report.gaps.len(), 0);
    }

    #[test]
    fn gaps_document_groups_items_by_directory() {
        let item = |name: &str, file: &str| SourceItem {
            name: name.into(),
            item_kind: "fn".into(),
            file: file.into(),
            line: 1,
            scanner: None,
        };
        let gaps = Gaps {
            total_pub_items: 3,
            covered_items: 1,
            by_directory: BTreeMap::from([
                (".".into(), vec![item("main", "main.rs")]),
                ("src".into(), vec![item("beta", "src/b.rs")]),
            ]),
            approximate: true,
            scanners_used: Vec::new(),
            scan_failures: Vec::new(),
        };
        let doc = serde_json::to_value(gaps.document()).unwrap();
        assert_eq!(doc["gap_count"], 2);
        assert_eq!(doc["by_directory"][1]["directory"], "src");
        assert_eq!(doc["by_directory"][1]["items"][0]["file"], "src/b.rs");
        assert_eq!(directory_of("main.rs"), ".");
    }
}
