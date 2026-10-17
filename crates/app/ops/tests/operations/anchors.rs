//! The anchors read view (ADR 0140): which entities `specforge-anchors.json` anchors to a source file, and which
//! source items it anchors to an entity.

use serde_json::json;
use specforge_ops::read::navigate::{FileMatch, anchors_of_entity, anchors_of_file};
use specforge_test::prelude::*;

use crate::view_support::{Assembled, Viewed};

/// A project declaring `behavior alpha`, whose anchors manifest anchors each `(entity, file)`.
fn anchored(anchors: &[(&str, &str)]) -> Assembled {
    let project = Assembled::over(specforge_registry::RegistryBuild::default())
        .source("behavior alpha \"A\" {\n}\n");
    let rows: Vec<_> = anchors
        .iter()
        .map(|(entity, file)| {
            json!({"entity_id": entity, "file": file, "line": 1, "symbol_name": entity,
                   "item_kind": "fn", "scanner": "manual"})
        })
        .collect();
    project.write(
        "specforge-anchors.json",
        json!({"version": 1, "anchors": rows}).to_string(),
    );
    project
}

/// An entity's anchors are the manifest's anchors of it, in manifest
/// order; an entity with none, or a project with no manifest, has none.
#[specforge_test(
    behavior = "provide_mcp_find_implementation_tool",
    verify = "an entity with no anchor has no implementations, and no anchors manifest is none"
)]
fn anchors_of_an_entity_are_in_manifest_order() {
    let project = anchored(&[
        ("alpha", "src/lib.rs"),
        ("beta", "src/lib.rs"),
        ("alpha", "src/net.rs"),
    ]);
    let files = |entity: &str| -> Vec<String> {
        anchors_of_entity(&project.view(), entity)
            .unwrap()
            .implementations
            .into_iter()
            .map(|implementation| implementation.file)
            .collect()
    };
    assert_eq!(files("alpha"), ["src/lib.rs", "src/net.rs"]);
    assert!(files("nope").is_empty());

    let bare = Assembled::over(specforge_registry::RegistryBuild::default())
        .source("behavior a \"A\" {\n}\n");
    assert_eq!(anchors_of_entity(&bare.view(), "a").unwrap().count, 0);
}

#[test]
fn the_tightest_match_wins() {
    let project = anchored(&[
        ("top", "a.rs"),
        ("nested", "sub/a.rs"),
        ("other", "sub/b.rs"),
        ("cache", "src/cache.rs"),
    ]);
    let found = |query: &str| {
        let found = anchors_of_file(&project.view(), query).unwrap();
        let ids: Vec<String> = found.entities.iter().map(|e| e.entity_id.clone()).collect();
        (found.match_mode, ids)
    };
    assert_eq!(found("a.rs"), (FileMatch::Exact, vec!["top".to_string()]));
    // Every spelling of a path is the path.
    for spelling in ["./a.rs", "a.rs", ".\\a.rs"] {
        assert_eq!(found(spelling).1, ["top"], "{spelling}");
    }
    assert_eq!(
        found("./sub"),
        (
            FileMatch::Directory,
            vec!["nested".to_string(), "other".to_string()]
        )
    );
    assert_eq!(
        found("b.rs"),
        (FileMatch::SuffixPath, vec!["other".to_string()])
    );
    // A substring is no match (C9-09).
    assert_eq!(found("e.rs").0, FileMatch::None);
    assert_eq!(found("c.rs").0, FileMatch::None);
}

#[test]
fn a_file_anchor_names_its_entitys_kind_when_the_graph_declares_it() {
    let project = anchored(&[("alpha", "src/a.rs"), ("ghost", "src/a.rs")]);
    let found = anchors_of_file(&project.view(), "src/a.rs").unwrap();
    assert_eq!(found.entities[0].kind.as_deref(), Some("behavior"));
    assert_eq!(found.entities[1].kind, None);
}
