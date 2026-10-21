//! Inference gaps: the public items of the project's source files that no
//! graph entity names (`specforge infer-status --gaps-detail`, the MCP
//! `specforge.infer_gaps` tool).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use specforge_common::shape::Shape;

use super::ScanFailure;
use super::manifest::InferenceManifest;
use super::scan::ScanOutcome;
use super::sources::InferenceSources;
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

/// The public items of the view's source files that no entity of its
/// graph names, scanned through its extensions' scanners in the view's
/// runtime. Without a root: `no_project`.
pub fn gaps(view: &ProjectView) -> Result<Gaps, OpError> {
    let root = view.project_root()?;
    let declarations = view.registries().declarations();
    let sources = InferenceSources::of(root, declarations, &InferenceManifest::at(root)?);
    let covered = crate::read::navigate::CoveredItems::of(view)?;
    let scanned =
        crate::read::infer::scan::scan_source_files(view.runtime().map(|r| r.as_ref()), &sources);
    Ok(Gaps::of(scanned, |item| {
        covered.covers(&item.file, &item.name)
    }))
}

/// The directory part of a project-relative path, `.` at the root.
pub fn directory_of(path: &str) -> &str {
    path.rfind('/').map_or(".", |i| &path[..i])
}

impl Gaps {
    /// What `scanned` found, less the items `covered` keeps out, by directory. Approximate when a
    /// scanner failed on a file.
    pub fn of(scanned: ScanOutcome, covered: impl Fn(&SourceItem) -> bool) -> Gaps {
        let total_pub_items = scanned.items.len();
        let mut by_directory: BTreeMap<String, Vec<SourceItem>> = BTreeMap::new();
        let mut uncovered = 0;
        for item in scanned.items {
            if covered(&item) {
                continue;
            }
            uncovered += 1;
            by_directory
                .entry(directory_of(&item.file).to_string())
                .or_default()
                .push(item);
        }
        Gaps {
            total_pub_items,
            covered_items: total_pub_items - uncovered,
            by_directory,
            approximate: !scanned.failures.is_empty(),
            scanners_used: scanned.scanners_used,
            scan_failures: scanned.failures,
        }
    }

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

    fn item(name: &str, kind: &str, line: usize) -> SourceItem {
        SourceItem {
            name: name.into(),
            item_kind: kind.into(),
            file: "src/lib.rs".into(),
            line,
            scanner: Some("rust".into()),
        }
    }

    fn scanned(items: Vec<SourceItem>) -> ScanOutcome {
        ScanOutcome {
            items,
            scanners_used: vec!["rust".into()],
            failures: Vec::new(),
        }
    }

    #[test]
    fn gaps_keep_the_items_no_entity_names() {
        let scanned = scanned(vec![
            item("hello", "function", 1),
            item("config", "struct", 2),
        ]);
        let gaps = Gaps::of(scanned, |item: &SourceItem| item.name == "hello");
        assert_eq!(gaps.total_pub_items, 2);
        assert_eq!(gaps.covered_items, 1);
        assert_eq!(gaps.by_directory["src"].len(), 1);
        assert_eq!(gaps.by_directory["src"][0].name, "config");
        assert!(!gaps.approximate);
        assert_eq!(gaps.scanners_used, vec!["rust"]);
    }

    #[test]
    fn gaps_of_covered_items_are_empty() {
        let gaps = Gaps::of(
            scanned(vec![item("hello", "function", 1)]),
            |item: &SourceItem| item.name == "hello",
        );
        assert_eq!(gaps.gap_count(), 0);
        assert_eq!(gaps.covered_items, 1);
    }

    #[test]
    fn gaps_of_no_items_count_nothing() {
        let gaps = Gaps::of(ScanOutcome::default(), |item: &SourceItem| {
            item.name == "hello"
        });
        assert_eq!(gaps.total_pub_items, 0);
        assert_eq!(gaps.gap_count(), 0);
    }

    #[test]
    fn a_scan_failure_makes_gaps_approximate() {
        let mut outcome = ScanOutcome::default();
        outcome.failures.push(ScanFailure {
            file: "a.rs".into(),
            error: specforge_wasm::CallError::new(
                specforge_wasm::Operation::Scan,
                "@specforge/rust",
                "scan__rust",
                specforge_wasm::CallFailure::NotLoaded,
            ),
        });
        assert!(Gaps::of(outcome, |_: &SourceItem| false).approximate);
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
