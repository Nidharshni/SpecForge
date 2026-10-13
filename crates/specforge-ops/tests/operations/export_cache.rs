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
fn an_export_reads_the_record_once() {
    use specforge_ops::export::{CacheWrite, Request, export_recorded};
    let project = recorded_project();
    std::fs::create_dir_all(cache_file(&project).parent().unwrap()).unwrap();
    std::fs::write(cache_file(&project), r#"{"schema": {"#).unwrap();

    let recorded = export_recorded(&project.view(), &Request::default());

    // One generation, one read: an unreadable record is reported once, and
    // the export records a new one.
    let codes: Vec<&str> = recorded
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(codes, ["I016"], "{:?}", recorded.diagnostics);
    assert_eq!(recorded.cache, CacheWrite::Written);
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache_file(&project)).unwrap()).unwrap();
    assert_eq!(record["schema"]["schema_version"]["major"], 1);
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

// ===========================================================================
// Every document names the project's versioned schema (plan 11, ADR 0015)
// ===========================================================================

/// A project whose versioned schema is 2.0.0: its cache recorded a kind the
/// project no longer has, a breaking change not yet recorded over.
fn project_at_two() -> crate::view_support::Project {
    use specforge_ops::export::{Request, export_recorded};
    let project = recorded_project();
    assert!(
        export_recorded(&project.view(), &Request::default())
            .export
            .is_ok()
    );
    let mut cache: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(cache_file(&project)).unwrap()).unwrap();
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "legacy", "source_extension": "x", "testable": false, "fields": []
        }));
    std::fs::write(cache_file(&project), cache.to_string()).unwrap();
    project
}

#[specforge_test(
    behavior = "render_model_json",
    verify = "output carries the schema_version specforge schema prints for the project"
)]

fn the_model_names_the_versioned_schema() {
    use specforge_ops::model::{ModelFormat, ModelOptions, model};
    let project = project_at_two();
    let json = |format| {
        model(
            &project.view(),
            &ModelOptions {
                format,
                ..ModelOptions::default()
            },
        )
        .unwrap()
        .document
    };

    let document: serde_json::Value = serde_json::from_str(&json(ModelFormat::Json)).unwrap();
    assert_eq!(document["schema_version"], "2.0.0", "{document}");
    assert!(!document.to_string().contains("model_"), "{document}");
    let dbml = json(ModelFormat::Dbml);
    assert_eq!(dbml.lines().nth(1), Some("// Schema version: 2.0.0"));
}

#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "every JSON export carries the project's schema version, whether the schema is attached or not"
)]
fn every_document_names_the_versioned_schema() {
    use specforge_ops::export::{Format, Request, Schema, export};
    use specforge_ops::trace::{Target, trace};
    let project = project_at_two();
    let view = project.view();
    let version = |request: Request| {
        let document: serde_json::Value =
            serde_json::from_str(&export(&view, &request).unwrap()).unwrap();
        document["schema_version"].clone()
    };

    for (what, request) in [
        ("graph", Request::default()),
        (
            "graph without the schema",
            Request {
                schema: Schema::Without,
                ..Request::default()
            },
        ),
        (
            "context",
            Request {
                format: Some(Format::Context),
                ..Request::default()
            },
        ),
        (
            "brief",
            Request {
                format: Some(Format::Brief),
                ..Request::default()
            },
        ),
        (
            "graph under a budget",
            Request {
                max_tokens: Some(100_000),
                ..Request::default()
            },
        ),
    ] {
        assert_eq!(version(request), "2.0.0", "{what}");
    }
    let traced = serde_json::to_value(trace(&view, Target::Every).unwrap()).unwrap();
    assert!(
        traced.to_string().contains("\"schema_version\":\"2.0.0\""),
        "{traced}"
    );
}

#[specforge_test(
    behavior = "serialize_traceability_data",
    verify = "the trace document carries the project's schema version"
)]
fn a_trace_carries_the_projects_schema_version() {
    use specforge_ops::trace::{Target, trace};
    let project = project_at_two();
    let view = project.view();
    let entity = serde_json::to_value(trace(&view, Target::Entity("b")).unwrap()).unwrap();
    assert_eq!(entity["schema_version"], "2.0.0", "{entity}");
    let chain = trace(&view, Target::Entity("b")).unwrap().into_chain();
    assert_eq!(chain.unwrap().schema_version, "2.0.0");
    let every = serde_json::to_value(trace(&view, Target::Every).unwrap()).unwrap();
    assert_eq!(every["schema_version"], "2.0.0", "{every}");
}
