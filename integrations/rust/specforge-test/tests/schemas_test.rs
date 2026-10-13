//! C11-10: the published report schemas must actually constrain the
//! producer. These tests serialize a real BinaryReport and validate it
//! against schema/specforge-binary-report.schema.json with a focused
//! JSON-Schema validator (type/required/properties/additionalProperties/
//! const/enum/items/$ref) — the subset these schemas use. Drift in either
//! direction (a new field, a renamed key) now fails a test instead of
//! silently breaking every consumer.

mod support;

use serde_json::Value;
use specforge_test::registry::{TestOutcome, TestRecordEntry};
use specforge_test::report;

fn schema_path(name: &str) -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .join("../../../schema")
        .join(name)
}

fn load_schema(name: &str) -> Value {
    let raw = std::fs::read_to_string(schema_path(name))
        .unwrap_or_else(|e| panic!("schema {name} must exist next to the repo root: {e}"));
    serde_json::from_str(&raw).unwrap()
}

/// Minimal draft-2020-12 subset validator for the keywords these schemas
/// use. Returns a list of violations (empty = valid).
fn validate(value: &Value, schema: &Value, root: &Value, path: &str, errors: &mut Vec<String>) {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let pointer = r.strip_prefix("#").unwrap_or(r);
        let target = root
            .pointer(pointer)
            .unwrap_or_else(|| panic!("unresolvable $ref {r}"));
        validate(value, target, root, path, errors);
        return;
    }

    if let Some(expected) = schema.get("const") {
        if value != expected {
            errors.push(format!(
                "{path}: const mismatch (expected {expected}, got {value})"
            ));
        }
    }

    if let Some(variants) = schema.get("enum").and_then(|v| v.as_array()) {
        if !variants.contains(value) {
            errors.push(format!("{path}: {value} not in enum {variants:?}"));
        }
    }

    let type_ok = match schema.get("type").and_then(|v| v.as_str()) {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some(t) => panic!("validator does not support type {t}"),
        None => true,
    };
    if !type_ok {
        errors.push(format!(
            "{path}: expected {}",
            schema["type"].as_str().unwrap_or("?")
        ));
        return;
    }

    if let (Some(props), Some(obj)) = (schema.get("properties"), value.as_object()) {
        for (key, sub) in props.as_object().unwrap() {
            if let Some(v) = obj.get(key) {
                validate(v, sub, root, &format!("{path}.{key}"), errors);
            }
        }
        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
            for key in obj.keys() {
                if !props.get(key).is_some() {
                    errors.push(format!(
                        "{path}: unexpected property '{key}' (additionalProperties: false)"
                    ));
                }
            }
        }
        for key in schema
            .get("required")
            .and_then(|r| r.as_array())
            .unwrap_or(&vec![])
            .clone()
        {
            let key = key.as_str().unwrap();
            if !obj.contains_key(key) {
                errors.push(format!("{path}: missing required property '{key}'"));
            }
        }
    }

    if let (Some(items), Some(arr)) = (schema.get("items"), value.as_array()) {
        for (i, v) in arr.iter().enumerate() {
            validate(v, items, root, &format!("{path}[{i}]"), errors);
        }
    }
}

fn make_entry(id: &str, outcome: TestOutcome) -> TestRecordEntry {
    TestRecordEntry {
        entity_kind: "behavior".to_string(),
        entity_id: id.to_string(),
        test_name: format!("test_{id}"),
        module_path: Some("specforge_test::cases".to_string()),
        file: "tests/x.rs".to_string(),
        verify: Some("rejects invalid input".to_string()),
        duration_ms: 3,
        outcome,
    }
}

#[test]
fn binary_report_conforms_to_published_schema() {
    let dir = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("auth_login", TestOutcome::Pass),
        make_entry("auth_logout", TestOutcome::Fail),
        {
            let mut e = make_entry("auth_locked", TestOutcome::Skipped);
            e.verify = None;
            e
        },
    ];
    report::write_report(dir.path(), "conformance", &entries).unwrap();

    let bytes = std::fs::read(dir.path().join("conformance.json")).unwrap();
    let report_json: Value = serde_json::from_slice(&bytes).unwrap();

    let schema = load_schema("specforge-binary-report.schema.json");
    let mut errors = Vec::new();
    validate(&report_json, &schema, &schema, "$", &mut errors);
    assert!(
        errors.is_empty(),
        "serialized report violates schema: {errors:?}"
    );

    // C11-04 fields are actually producible end-to-end. Reports are
    // sorted by (entity_id, test_name) — look entries up by id.
    let by_id = |id: &str| {
        report_json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["entity_id"] == id)
            .unwrap()
            .clone()
    };
    let passing = by_id("auth_login");
    assert_eq!(passing["status"], "pass");
    assert_eq!(passing["module_path"], "specforge_test::cases");
    assert_eq!(passing["duration_ms"], 3);
    let skipped = by_id("auth_locked");
    assert_eq!(skipped["status"], "skipped");
}

#[specforge_test_macros::test(
    behavior = "record_test_via_drop_guard",
    verify = "a recorded entry conforms to the published binary report schema"
)]
fn a_recorded_report_conforms_to_the_published_schema() {
    let reports = tempfile::tempdir().unwrap();
    support::run_fixture(&["--include-ignored"], reports.path());
    let schema = load_schema("specforge-binary-report.schema.json");
    let mut checked = 0;
    for file in std::fs::read_dir(reports.path()).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let report: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert!(report["entries"][0]["module_path"].is_string(), "{report}");
            let mut errors = Vec::new();
            validate(&report, &schema, &schema, "$", &mut errors);
            assert!(errors.is_empty(), "{}: {errors:?}", path.display());
            checked += 1;
        }
    }
    assert_eq!(checked, 1, "one report per fixture run");
}

#[test]
fn schema_rejects_unknown_fields_and_bad_statuses() {
    let schema = load_schema("specforge-binary-report.schema.json");

    let bad_status = serde_json::json!({
        "schema_version": "1.0",
        "binary_name": "b",
        "entries": [{
            "entity_kind": "behavior",
            "entity_id": "e",
            "test_name": "t",
            "file": "f.rs",
            "status": "flaky"
        }]
    });
    let mut errors = Vec::new();
    validate(&bad_status, &schema, &schema, "$", &mut errors);
    assert!(!errors.is_empty(), "status 'flaky' must be rejected");

    let unknown_field = serde_json::json!({
        "schema_version": "1.0",
        "binary_name": "b",
        "entries": [{
            "entity_kind": "behavior",
            "entity_id": "e",
            "test_name": "t",
            "file": "f.rs",
            "status": "pass",
            "mystery_field": 1
        }]
    });
    let mut errors = Vec::new();
    validate(&unknown_field, &schema, &schema, "$", &mut errors);
    assert!(
        errors.iter().any(|e| e.contains("mystery_field")),
        "additionalProperties:false must flag unknown fields: {errors:?}"
    );
}

#[test]
fn adapter_report_schema_is_self_consistent() {
    // specforge-report.schema.json is consumed by external adapters; at
    // minimum it must parse and its internal $refs must resolve.
    let schema = load_schema("specforge-report.schema.json");
    let defs = schema.get("$defs").and_then(|d| d.as_object()).unwrap();
    for (name, def) in defs {
        for r in find_refs(def) {
            let local = r.strip_prefix("#/$defs/").unwrap();
            assert!(
                defs.contains_key(local),
                "$defs.{name} references unknown $defs.{local}"
            );
        }
    }
}

fn find_refs(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => map
            .iter()
            .flat_map(|(k, v)| {
                if k == "$ref" {
                    vec![v.as_str().unwrap_or_default().to_string()]
                } else {
                    find_refs(v)
                }
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(find_refs).collect(),
        _ => vec![],
    }
}
