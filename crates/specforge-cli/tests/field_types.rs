//! Field values checked against the type their extension declares:
//! `specforge check` reports values that can't be that type (E061), and
//! `specforge export` carries the coerced value.

use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;

const GOVERNANCE_CONFIG: &str = r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/governance"]}"#;

fn governance_project(spec: &str) -> tempfile::TempDir {
    setup_project_with_config(GOVERNANCE_CONFIG, &[("main.spec", spec)])
}

/// `specforge check --format=json`: the exit code and the diagnostics.
fn check(dir: &tempfile::TempDir) -> (Option<i32>, Vec<serde_json::Value>) {
    let output = specforge_cmd()
        .args(["check", "--format=json"])
        .arg(dir.path())
        .output()
        .unwrap();
    let diags = parse_json_stdout(&output)
        .as_array()
        .expect("check prints a JSON array")
        .clone();
    (output.status.code(), diags)
}

/// The exported fields of entity `id`.
fn exported_fields(dir: &tempfile::TempDir, id: &str) -> serde_json::Value {
    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let export = parse_json_stdout(&output);
    export["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap_or_else(|| panic!("no node '{id}' in {export}"))["fields"]
        .clone()
}

const FAILURE_MODE_FIELDS: &str = r#"cause "disk full"
  effect "writes fail"
  severity high"#;

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a single string on a string_list field becomes a one-item list"
)]
fn a_single_string_on_a_string_list_field_is_exported_as_a_list() {
    let dir = governance_project(
        r#"decision d1 "D" {
  status accepted
  context "c"
  decision "x"
  consequences "just a string"
}
"#,
    );

    let (code, diags) = check(&dir);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert_eq!(code, Some(0));
    assert_eq!(
        exported_fields(&dir, "d1")["consequences"],
        serde_json::json!(["just a string"])
    );
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a single reference on a reference_list field becomes a one-item list"
)]
fn a_single_reference_on_a_reference_list_field_links_like_a_list() {
    let dir = governance_project(
        r#"invariant inv1 "I" {
  guarantee "holds"
}
behavior b1 "B" {
  contract "keeps inv1"
  category command
  invariants [inv1]
}
decision d1 "D" {
  status accepted
  context "c"
  decision "x"
  invariants inv1
}
"#,
    );

    let (code, diags) = check(&dir);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert_eq!(code, Some(0));
    assert_eq!(
        exported_fields(&dir, "d1")["invariants"],
        serde_json::json!(["inv1"])
    );

    // The single reference is an edge, exactly as `invariants [inv1]` is.
    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg(dir.path())
        .output()
        .unwrap();
    let export = parse_json_stdout(&output);
    let edges: Vec<(String, String, String)> = export["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap().to_string(),
                e["target"].as_str().unwrap().to_string(),
                e["label"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        edges.contains(&("d1".into(), "inv1".into(), "invariants".into())),
        "{edges:?}"
    );
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a value that is not the declared integer, bool or enum type is an error"
)]
fn a_value_that_is_not_an_integer_on_an_integer_field_is_an_error() {
    let dir = governance_project(&format!(
        r#"failure_mode fm1 "F" {{
  {FAILURE_MODE_FIELDS}
  rpn "abc"
}}
"#
    ));

    let (code, diags) = check(&dir);
    assert_eq!(code, Some(1), "{diags:?}");
    assert_eq!(diags.len(), 1, "{diags:?}");
    let diag = &diags[0];
    assert_eq!(diag["code"], "E061");
    assert_eq!(diag["severity"], "Error");
    assert_eq!(
        diag["message"],
        "field 'rpn' of failure_mode 'fm1' is declared integer, but was given \"abc\""
    );
    // The span is the value's, not the entity's.
    assert_eq!(diag["span"]["start_line"], 5);
    assert_eq!(diag["span"]["start_col"], 7);
    assert_eq!(diag["span"]["end_line"], 5);
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a value that is not the declared integer, bool or enum type is an error"
)]
fn a_quoted_integer_on_an_integer_field_is_exported_as_a_number() {
    let dir = governance_project(&format!(
        r#"failure_mode fm1 "F" {{
  {FAILURE_MODE_FIELDS}
  rpn "12"
}}
failure_mode fm2 "G" {{
  {FAILURE_MODE_FIELDS}
  rpn 40
}}
"#
    ));

    let (code, diags) = check(&dir);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert_eq!(code, Some(0));
    assert_eq!(exported_fields(&dir, "fm1")["rpn"], serde_json::json!(12));
    assert_eq!(exported_fields(&dir, "fm2")["rpn"], serde_json::json!(40));
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a list on a field declared as a single value is an error"
)]
fn a_list_on_a_string_field_is_an_error() {
    let dir = governance_project(
        r#"decision d1 "D" {
  status accepted
  context ["one", "two"]
  decision "x"
}
"#,
    );

    let (code, diags) = check(&dir);
    assert_eq!(code, Some(1), "{diags:?}");
    let e061: Vec<_> = diags.iter().filter(|d| d["code"] == "E061").collect();
    assert_eq!(e061.len(), 1, "{diags:?}");
    assert_eq!(
        e061[0]["message"],
        "field 'context' of decision 'd1' is declared string, but was given a list"
    );
    assert_eq!(e061[0]["span"]["start_line"], 3);
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a single integer, boolean or date, or a mixed list, on a list field becomes a list of strings"
)]
fn a_single_integer_or_a_mixed_list_on_a_list_field_is_exported_as_a_list() {
    let dir = governance_project(
        r#"decision d1 "D" {
  status accepted
  context "c"
  decision "x"
  consequences 5
}
decision d2 "E" {
  status accepted
  context "c"
  decision "x"
  consequences ["a", 7]
}
"#,
    );

    let (code, diags) = check(&dir);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert_eq!(code, Some(0));
    assert_eq!(
        exported_fields(&dir, "d1")["consequences"],
        serde_json::json!(["5"])
    );
    assert_eq!(
        exported_fields(&dir, "d2")["consequences"],
        serde_json::json!(["a", "7"])
    );
}
