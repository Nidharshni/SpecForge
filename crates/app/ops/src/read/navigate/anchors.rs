//! The anchors read view: which source items `<root>/specforge-anchors.json` (written by inference
//! tooling) anchors to an entity, which entities it anchors to a source file, and which source items
//! it covers (inference gaps, ADR 0199). The manifest is read
//! once per question through the one manifest reader (`read::infer::read_manifest`, ADR 0112 D3) and is
//! private to this module; MCP's `specforge.find_implementation`, `specforge.find_spec_for_source` and
//! the infer prompt's file scope render these documents (ADR 0140).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specforge_common::shape::Shape;

use super::{FileMatch, match_file};
use crate::read::infer::SourceKey;
use crate::view::{EntityField, ProjectView, not_found};
use crate::{OpError, Refusal};

/// The source anchors manifest's file, at the project root.
const ANCHORS_FILENAME: &str = "specforge-anchors.json";

/// What the anchors manifest anchors to the source files `file_path` names
/// (`McpSpecForSourceResult`; the infer prompt's file scope lists its `entities`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct FileAnchors {
    /// The query, as asked.
    pub file_path: String,
    /// `exact` when some anchor's file is the query; else `directory` when one is under it, else
    /// `suffix_path`; `none` when none matches ([`match_file`]).
    pub match_mode: FileMatch,
    /// The anchors of the tightest non-empty mode, in manifest order.
    pub entities: Vec<AnchoredEntity>,
    pub count: usize,
}

/// One entity anchored to a source file (`McpSpecForSourceEntity`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct AnchoredEntity {
    pub entity_id: String,
    /// The entity's kind; absent when the graph does not declare the id (a stale anchor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub file: String,
    pub line: usize,
    pub symbol_name: String,
    pub item_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// The source items the anchors manifest anchors one entity to (`McpImplementationResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct EntityAnchors {
    pub entity_id: String,
    /// In manifest order.
    pub implementations: Vec<Implementation>,
    pub count: usize,
}

/// One source item an entity is anchored to (`McpImplementation`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct Implementation {
    pub file: String,
    pub line: usize,
    pub symbol_name: String,
    pub item_kind: String,
    pub scanner: String,
}

#[derive(Debug, Default, Deserialize)]
struct AnchorManifest {
    // Required by the manifest's shape; no version rule (E071 checks only specforge-infer.json's).
    #[expect(
        dead_code,
        reason = "required by the manifest's shape, read by no rule"
    )]
    version: u32,
    #[serde(default)]
    anchors: Vec<SourceAnchor>,
}

#[derive(Debug, Deserialize)]
struct SourceAnchor {
    entity_id: String,
    file: String,
    line: usize,
    symbol_name: String,
    item_kind: String,
    scanner: String,
    #[serde(default)]
    confidence: Option<f64>,
}

/// The view's anchors manifest: empty without the file; `no_project` without a root; E071 when the file
/// cannot be used.
fn manifest(view: &ProjectView<'_>) -> Result<AnchorManifest, OpError> {
    let root = view.project_root()?;
    Ok(
        crate::read::infer::read_manifest::<AnchorManifest>(root, ANCHORS_FILENAME)?
            .unwrap_or_default(),
    )
}

/// Which public source items the project covers (CONTEXT.md "Covered source item", ADR 0199): an item
/// an anchor names, by its file and its name, for an entity the view's graph declares; else an item
/// whose name is the id of a declared entity no anchor names. What inference gaps leave out.
pub(crate) struct CoveredItems<'a> {
    /// Every id the graph declares.
    declared: HashSet<&'a str>,
    /// Source key to the symbol names anchored there to a declared entity.
    anchored: HashMap<String, HashSet<String>>,
    /// The declared entities with at least one such anchor: they cover only where they are anchored.
    anchored_entities: HashSet<&'a str>,
}

impl<'a> CoveredItems<'a> {
    /// The view's covered items: `no_project` without a root, E071 when the anchors manifest cannot
    /// be used, the name rule alone when there is none.
    pub(crate) fn of(view: &ProjectView<'a>) -> Result<Self, OpError> {
        let manifest = manifest(view)?;
        let declared: HashSet<&str> = view
            .graph()
            .nodes()
            .into_iter()
            .map(|n| n.id.raw.as_str())
            .collect();
        let mut anchored: HashMap<String, HashSet<String>> = HashMap::new();
        let mut anchored_entities = HashSet::new();
        for anchor in &manifest.anchors {
            // A stale anchor (an id the graph lacks) and an anchor outside the root cover nothing;
            // navigation still lists them (ADR 0140 D3).
            let Some(&entity) = declared.get(anchor.entity_id.as_str()) else {
                continue;
            };
            let Ok(key) = SourceKey::of(&anchor.file) else {
                continue;
            };
            anchored
                .entry(key.as_str().to_string())
                .or_default()
                .insert(anchor.symbol_name.clone());
            anchored_entities.insert(entity);
        }
        Ok(CoveredItems {
            declared,
            anchored,
            anchored_entities,
        })
    }

    /// Whether the item `name` in the source `file` (a source key, as a scan reports it) is covered.
    pub(crate) fn covers(&self, file: &str, name: &str) -> bool {
        self.anchored
            .get(file)
            .is_some_and(|names| names.contains(name))
            || (self.declared.contains(name) && !self.anchored_entities.contains(name))
    }
}

/// Which entities belong to the source files `file_path` names: the manifest's anchors in them, under
/// [`match_file`], each with its entity's kind when the view's graph declares the id. The tightest
/// non-empty mode wins: exact matches, else those under the query or ending with it. Rows are in
/// manifest order. No file is opened but the manifest: `file_path` is compared as text. A missing
/// manifest anchors nothing; one that cannot be used is E071; a view with no root is `no_project`.
pub fn anchors_of_file(view: &ProjectView<'_>, file_path: &str) -> Result<FileAnchors, OpError> {
    let manifest = manifest(view)?;
    let mut exact = Vec::new();
    let mut loose = Vec::new();
    let mut under = false;
    for anchor in manifest.anchors {
        match match_file(file_path, &anchor.file) {
            FileMatch::Exact => exact.push(anchor),
            FileMatch::Directory => {
                under = true;
                loose.push(anchor);
            }
            FileMatch::SuffixPath => loose.push(anchor),
            FileMatch::None => {}
        }
    }
    let (match_mode, chosen) = if !exact.is_empty() {
        (FileMatch::Exact, exact)
    } else if loose.is_empty() {
        (FileMatch::None, loose)
    } else if under {
        (FileMatch::Directory, loose)
    } else {
        (FileMatch::SuffixPath, loose)
    };
    let graph = view.graph();
    let entities: Vec<AnchoredEntity> = chosen
        .into_iter()
        .map(|anchor| AnchoredEntity {
            kind: graph
                .node(&anchor.entity_id)
                .map(|node| node.kind.raw.to_string()),
            entity_id: anchor.entity_id,
            file: anchor.file,
            line: anchor.line,
            symbol_name: anchor.symbol_name,
            item_kind: anchor.item_kind,
            confidence: anchor.confidence,
        })
        .collect();
    Ok(FileAnchors {
        file_path: file_path.to_string(),
        match_mode,
        count: entities.len(),
        entities,
    })
}

/// The source items the manifest anchors `entity_id` to, in manifest order; none for an entity the graph
/// declares with no anchor, or a project with no manifest. An id that neither the graph declares nor the
/// manifest anchors is E003 (with a did-you-mean); the anchors of an id the graph no longer declares (a
/// stale anchor) are still listed. A manifest that cannot be used is E071, before any id is judged.
pub fn anchors_of_entity(
    view: &ProjectView<'_>,
    entity_id: &str,
) -> Result<EntityAnchors, Refusal<EntityField>> {
    let manifest = manifest(view)?;
    let implementations: Vec<Implementation> = manifest
        .anchors
        .into_iter()
        .filter(|anchor| anchor.entity_id == entity_id)
        .map(|anchor| Implementation {
            file: anchor.file,
            line: anchor.line,
            symbol_name: anchor.symbol_name,
            item_kind: anchor.item_kind,
            scanner: anchor.scanner,
        })
        .collect();
    if implementations.is_empty() && view.graph().node(entity_id).is_none() {
        return Err(Refusal::about(
            EntityField::EntityId,
            not_found(view.graph(), entity_id),
        ));
    }
    Ok(EntityAnchors {
        entity_id: entity_id.to_string(),
        count: implementations.len(),
        implementations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Viewed;
    use specforge_project::testing::Assembled;

    #[test]
    fn the_manifest_is_read_from_the_root() {
        let fixture = Assembled::new();
        std::fs::write(
            fixture.root().join(ANCHORS_FILENAME),
            r#"{"version":1,"anchors":[
                {"entity_id":"a","file":"src/orders.rs","line":25,"symbol_name":"a","item_kind":"fn","scanner":"s"},
                {"entity_id":"b","file":"src/validation.rs","line":5,"symbol_name":"b","item_kind":"fn","scanner":"s"}]}"#,
        )
        .unwrap();

        assert_eq!(anchors_of_entity(&fixture.view(), "a").unwrap().count, 1);
        assert_eq!(anchors_of_file(&fixture.view(), "src").unwrap().count, 2);
    }

    #[test]
    fn no_manifest_is_no_anchors() {
        let fixture = Assembled::new();
        assert_eq!(
            anchors_of_file(&fixture.view(), "src/a.rs").unwrap().count,
            0
        );
        // With no manifest, an id the graph lacks is not found.
        let refused = anchors_of_entity(&fixture.view(), "a").unwrap_err();
        assert_eq!(refused.error.code, "E003");
    }

    #[test]
    fn an_unusable_file_is_refused_by_why_not_by_text() {
        let fixture = Assembled::new();
        let path = fixture.root().join(ANCHORS_FILENAME);
        std::fs::write(&path, r#"{"version":1,"anchors":[{"entity_id":"x"}]}"#).unwrap();
        let invalid = anchors_of_file(&fixture.view(), "x").unwrap_err();
        assert_eq!(invalid.kind, crate::OpErrorKind::SchemaMismatch);
        assert!(
            invalid
                .message
                .starts_with("failed to parse specforge-anchors.json: missing field `file`"),
            "{}",
            invalid.message
        );

        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let unreadable = anchors_of_file(&fixture.view(), "x").unwrap_err();
        assert_eq!(unreadable.kind, crate::OpErrorKind::Internal);
        assert!(
            unreadable
                .message
                .starts_with("failed to read specforge-anchors.json")
        );
    }

    /// A project declaring three behaviors, with `anchors` as its anchors manifest.
    fn covering(anchors: Option<&str>) -> Assembled {
        let fixture = Assembled::over(specforge_registry::RegistryBuild::default()).source(
            "behavior compiled_project \"C\" {\n}\nbehavior compile_project \"P\" {\n}\nbehavior helper \"H\" {\n}\n",
        );
        if let Some(anchors) = anchors {
            fixture.write(ANCHORS_FILENAME, anchors);
        }
        fixture
    }

    fn anchor(entity: &str, file: &str, symbol: &str) -> String {
        format!(
            r#"{{"entity_id":"{entity}","file":"{file}","line":1,"symbol_name":"{symbol}","item_kind":"fn","scanner":"s"}}"#
        )
    }

    fn manifest_of(rows: &[String]) -> String {
        format!(r#"{{"version":1,"anchors":[{}]}}"#, rows.join(","))
    }

    #[test]
    fn an_anchor_covers_its_file_and_symbol_and_nothing_else() {
        let fixture = covering(Some(&manifest_of(&[anchor(
            "compiled_project",
            "src/lib.rs",
            "CompiledProject",
        )])));
        let covered = CoveredItems::of(&fixture.view()).unwrap();
        assert!(covered.covers("src/lib.rs", "CompiledProject"));
        assert!(!covered.covers("src/other.rs", "CompiledProject"));
        assert!(!covered.covers("src/lib.rs", "Other"));
    }

    #[test]
    fn an_anchored_entity_no_longer_covers_by_name() {
        let fixture = covering(Some(&manifest_of(&[anchor(
            "compile_project",
            "src/lib.rs",
            "compile_project",
        )])));
        let covered = CoveredItems::of(&fixture.view()).unwrap();
        assert!(covered.covers("src/lib.rs", "compile_project"));
        assert!(!covered.covers("src/other.rs", "compile_project"));
    }

    #[test]
    fn an_unanchored_entity_covers_by_name_and_without_a_manifest_names_decide() {
        let fixture = covering(Some(&manifest_of(&[anchor(
            "compile_project",
            "src/lib.rs",
            "compile_project",
        )])));
        assert!(
            CoveredItems::of(&fixture.view())
                .unwrap()
                .covers("any/x.rs", "helper")
        );
        let bare = covering(None);
        let covered = CoveredItems::of(&bare.view()).unwrap();
        assert!(covered.covers("x.rs", "compile_project"));
        assert!(!covered.covers("x.rs", "undeclared"));
    }

    #[test]
    fn a_stale_or_outside_anchor_covers_nothing_and_a_dotted_file_is_its_key() {
        let fixture = covering(Some(&manifest_of(&[
            anchor("ghost", "src/a.rs", "Other"),
            anchor("compiled_project", "../x.rs", "CompiledProject"),
            anchor("compile_project", "./src/lib.rs", "compile_project"),
        ])));
        let covered = CoveredItems::of(&fixture.view()).unwrap();
        assert!(!covered.covers("src/a.rs", "Other"));
        // `compiled_project` has no valid anchor, so it covers by name.
        assert!(covered.covers("y.rs", "compiled_project"));
        assert!(covered.covers("src/lib.rs", "compile_project"));
    }
}
