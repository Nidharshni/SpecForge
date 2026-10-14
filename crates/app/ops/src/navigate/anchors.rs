//! The source anchors manifest: `<root>/specforge-anchors.json`, which
//! source item each entity is anchored to (written by inference tooling,
//! read by navigation).

use serde::{Deserialize, Serialize};

use crate::OpError;
use crate::view::ProjectView;

/// The source anchors manifest's file, at the project root.
pub const ANCHORS_FILENAME: &str = "specforge-anchors.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorManifest {
    pub version: u32,
    #[serde(default)]
    pub anchors: Vec<SourceAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAnchor {
    pub entity_id: String,
    pub file: String,
    pub line: usize,
    pub symbol_name: String,
    pub item_kind: String,
    pub scanner: String,
    #[serde(default)]
    pub mapping_strategy: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

impl Default for AnchorManifest {
    fn default() -> Self {
        Self {
            version: 1,
            anchors: Vec::new(),
        }
    }
}

/// The anchors manifest of the view's project: empty without a root or
/// without the file; a refusal when the file cannot be used.
pub fn source_anchors(view: &ProjectView<'_>) -> Result<AnchorManifest, OpError> {
    let Some(root) = view.root() else {
        return Ok(AnchorManifest::default());
    };
    Ok(crate::infer::read_manifest::<AnchorManifest>(root, ANCHORS_FILENAME)?.unwrap_or_default())
}

/// The anchors of `entity_id`, in manifest order.
pub fn anchors_of_entity<'m>(
    manifest: &'m AnchorManifest,
    entity_id: &str,
) -> Vec<&'m SourceAnchor> {
    manifest
        .anchors
        .iter()
        .filter(|anchor| anchor.entity_id == entity_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::testing::Fixture;

    fn sample_anchor(entity: &str, file: &str, line: usize) -> SourceAnchor {
        SourceAnchor {
            entity_id: entity.into(),
            file: file.into(),
            line,
            symbol_name: entity.into(),
            item_kind: "function".into(),
            scanner: "@specforge/rust".into(),
            mapping_strategy: Some("exact_snake_case".into()),
            confidence: Some(0.8),
        }
    }

    #[test]
    fn default_manifest_is_empty() {
        let m = AnchorManifest::default();
        assert_eq!(m.version, 1);
        assert!(m.anchors.is_empty());
    }

    #[test]
    fn round_trip_serialization() {
        let m = AnchorManifest {
            anchors: vec![sample_anchor("handle_login", "src/auth.rs", 10)],
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&m).unwrap();
        let loaded: AnchorManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.anchors.len(), 1);
        assert_eq!(loaded.anchors[0].entity_id, "handle_login");
    }

    #[test]
    fn source_anchors_reads_the_file() {
        let fixture = Fixture::new();
        let m = AnchorManifest {
            anchors: vec![
                sample_anchor("process_order", "src/orders.rs", 25),
                sample_anchor("validate_input", "src/validation.rs", 5),
            ],
            ..Default::default()
        };
        std::fs::write(
            fixture.dir.path().join(ANCHORS_FILENAME),
            serde_json::to_string_pretty(&m).unwrap(),
        )
        .unwrap();

        let loaded = source_anchors(&fixture.view()).unwrap();
        assert_eq!(loaded.anchors.len(), 2);
    }

    #[test]
    fn source_anchors_is_empty_without_a_file_or_a_root() {
        let fixture = Fixture::new();
        assert!(source_anchors(&fixture.view()).unwrap().anchors.is_empty());
        assert!(
            source_anchors(&fixture.rootless_view())
                .unwrap()
                .anchors
                .is_empty()
        );
    }

    #[test]
    fn an_unusable_file_is_refused_by_why_not_by_text() {
        let fixture = Fixture::new();
        let path = fixture.dir.path().join(ANCHORS_FILENAME);
        std::fs::write(&path, r#"{"version":1,"anchors":[{"entity_id":"x"}]}"#).unwrap();
        let invalid = source_anchors(&fixture.view()).unwrap_err();
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
        let unreadable = source_anchors(&fixture.view()).unwrap_err();
        assert_eq!(unreadable.kind, crate::OpErrorKind::Internal);
        assert!(
            unreadable
                .message
                .starts_with("failed to read specforge-anchors.json")
        );
    }
}
