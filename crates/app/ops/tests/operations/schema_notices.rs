//! Every read outcome that names the schema's version carries what
//! versioning it against the root's record found (I016, W053; ADR 0111).

use crate::view_support::{Assembled, Viewed};
use specforge_common::Diagnostic;
use specforge_ops::read::export::{Request, export, export_recorded};
use specforge_ops::read::query::{QueryRequest, query};
use specforge_test::prelude::*;

fn project() -> Assembled {
    Assembled::over(crate::view_support::registries(&["behavior"], &[]))
        .source("behavior b \"B\" {\n  contract \"The system MUST work\"\n}\n")
}

fn record(project: &Assembled) -> std::path::PathBuf {
    project.root().join(".specforge/schema-cache.json")
}

/// A project whose record cannot be read.
fn unreadable() -> Assembled {
    let project = project();
    std::fs::create_dir_all(record(&project).parent().unwrap()).unwrap();
    std::fs::write(record(&project), r#"{"schema": {"#).unwrap();
    project
}

/// A project whose record holds a kind the project lacks: one W053 per
/// breaking change.
fn breaking() -> Assembled {
    let project = project();
    assert!(
        export_recorded(&project.view(), &Request::default())
            .export
            .is_ok()
    );
    let mut cache: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(record(&project)).unwrap()).unwrap();
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "legacy", "source_extension": "x", "testable": false, "fields": []
        }));
    std::fs::write(record(&project), cache.to_string()).unwrap();
    project
}

fn codes(notices: &[Diagnostic]) -> Vec<&str> {
    notices.iter().map(|d| d.code.as_str()).collect()
}

fn b() -> QueryRequest<'static> {
    QueryRequest {
        entity_id: "b",
        ..QueryRequest::default()
    }
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn an_export_and_a_query_carry_the_records_notices() {
    let project = unreadable();
    let view = project.view();
    let exported = export(&view, &Request::default()).unwrap();
    assert_eq!(codes(&exported.notices), ["I016"]);
    assert_eq!(codes(&query(&view, &b()).unwrap().notices), ["I016"]);

    let project = breaking();
    let view = project.view();
    let exported = export(&view, &Request::default()).unwrap();
    let queried = query(&view, &b()).unwrap();
    assert_eq!(codes(&exported.notices), ["W053"]);
    assert_eq!(queried.notices, exported.notices);
    // The recorded export reports the same, and records after.
    assert_eq!(
        export_recorded(&view, &Request::default()).notices,
        exported.notices
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn the_schema_and_its_publication_carry_the_records_notices() {
    use specforge_ops::read::export::Format;
    use specforge_ops::read::schema::{SchemaRequest, json_schema, schema};
    let project = unreadable();
    let view = project.view();

    let outcome = schema(&view, &SchemaRequest::default()).unwrap();
    assert_eq!(codes(&outcome.notices), ["I016"]);
    let document = serde_json::to_value(&outcome).unwrap();
    assert!(document.get("notices").is_none(), "{document}");
    assert_eq!(
        codes(&json_schema(&view, Format::Graph).unwrap().notices),
        ["I016"]
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "every reader of the schema reports the notices export reports for the same record"
)]
fn a_trace_and_the_model_carry_the_records_notices() {
    use specforge_ops::read::model::{ModelOptions, model};
    use specforge_ops::read::trace::{Target, trace};
    let project = unreadable();
    let view = project.view();

    let traced = trace(&view, Target::Entity("b")).unwrap();
    assert_eq!(codes(&traced.notices), ["I016"]);
    let document = serde_json::to_value(&traced).unwrap();
    assert!(document.get("notices").is_none(), "{document}");

    let modelled = model(&view, &ModelOptions::default()).unwrap();
    assert_eq!(codes(&modelled.notices), ["I016"]);
    let filtered = ModelOptions {
        kinds: vec!["ghost".into()],
        ..ModelOptions::default()
    };
    // The schema's notices come before the request's (D7).
    assert_eq!(
        codes(&model(&view, &filtered).unwrap().notices),
        ["I016", "I020"]
    );
}
