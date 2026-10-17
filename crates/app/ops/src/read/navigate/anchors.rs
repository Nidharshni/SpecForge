//! The anchors read view: which source items `<root>/specforge-anchors.json` (written by inference
//! tooling) anchors to an entity, and which entities it anchors to a source file. The manifest is read
//! once per question through the one manifest reader (`read::infer::read_manifest`, ADR 0112 D3) and is
//! private to this module; MCP's `specforge.find_implementation`, `specforge.find_spec_for_source` and
//! the infer prompt's file scope render these documents (ADR 0140).

use serde::{Deserialize, Serialize};
use specforge_common::shape::Shape;

use super::{FileMatch, match_file};
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
}
