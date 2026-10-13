//! `specforge export` records the schema cache at the view's root, and a
//! failed export leaves it alone.

use specforge_test::prelude::*;

// ===========================================================================
// `specforge export` records the cache: the operation (plan 02, ADR 0029 D7)
// ===========================================================================

fn recorded_project() -> crate::view_support::Project {
    crate::view_support::Project::new(
        "behavior b \"B\" {\n  contract \"The system MUST work\"\n}\n",
        crate::view_support::registries(&["behavior"], &[]),
    )
}

fn cache_file(project: &crate::view_support::Project) -> std::path::PathBuf {
    project.dir.path().join(".specforge/schema-cache.json")
}

#[specforge_test(
    behavior = "read_views_over_the_project_view",
    verify = "the schema cache is the view root's, never an ancestor's"
)]
fn export_recorded_compares_then_records_on_success() {
    use specforge_ops::export::{CacheWrite, Request, export_recorded};
    let project = recorded_project();
    let request = Request::default();

    // No cache yet: nothing breaks, and the export records it.
    let first = export_recorded(&project.view(), &request);
    assert!(first.export.is_ok());
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.cache, CacheWrite::Written);
    assert!(
        cache_file(&project).is_file(),
        "recorded at the view's root"
    );

    // A cached schema that had a kind this project no longer has: breaking
    // (W053), found before the export, and the export records the new one.
    let mut cache: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache_file(&project)).unwrap()).unwrap();
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "legacy", "source_extension": "x", "testable": false, "fields": []
        }));
    std::fs::write(cache_file(&project), cache.to_string()).unwrap();
    let second = export_recorded(&project.view(), &request);
    assert!(second.export.is_ok());
    assert!(
        second.diagnostics.iter().any(|d| d.code == "W053"),
        "{:?}",
        second.diagnostics
    );
    assert_eq!(second.cache, CacheWrite::Written);

    // Recorded: the next export compares against what it found.
    let third = export_recorded(&project.view(), &request);
    assert!(third.diagnostics.is_empty(), "{:?}", third.diagnostics);
}

#[test]
fn a_failed_export_leaves_the_cache() {
    use specforge_ops::export::{CacheWrite, Request, export_recorded};
    let project = recorded_project();
    let request = Request {
        scope: Some("nope"),
        ..Request::default()
    };

    let recorded = export_recorded(&project.view(), &request);

    assert!(recorded.export.is_err());
    assert_eq!(recorded.cache, CacheWrite::NotWritten);
    assert!(
        !cache_file(&project).exists(),
        "a failed export records nothing"
    );
}
