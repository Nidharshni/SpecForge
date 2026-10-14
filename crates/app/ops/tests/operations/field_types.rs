//! Field values checked against the type their extension declares:
//! `specforge check` reports values that can't be that type (E061), and
//! `specforge export` carries the coerced value.

use super::compiled_support::{TestProject, check_document, checked, command_document, exported};
use serde_json::{Value, json};
use specforge_ops::analysis::check::CheckOptions;
use specforge_project::CompiledProject;
use specforge_test_macros::test as specforge_test;

/// A project enabling software and governance, `spec` as its one file under `spec/`.
fn governance_project(spec: &str) -> (TestProject, CompiledProject) {
    project_enabling(&["@specforge/software", "@specforge/governance"], spec)
}

/// A project enabling `extensions`, `spec` as its one file under `spec/`.
fn project_enabling(extensions: &[&str], spec: &str) -> (TestProject, CompiledProject) {
    let project = TestProject::new()
        .enabling(extensions)
        .config(|c| c["spec_root"] = "spec".into())
        .file("spec/main.spec", spec);
    let compiled = project.compile();
    (project, compiled)
}

/// `specforge check --format=json`: whether it passes (exit 0) and the diagnostics.
fn check(compiled: &CompiledProject) -> (bool, Vec<Value>) {
    let ok = checked(compiled, CheckOptions::default()).ok();
    (ok, check_document(compiled, CheckOptions::default()))
}

/// The exported fields of entity `id`.
fn exported_fields(compiled: &CompiledProject, id: &str) -> Value {
    let export = exported(compiled);
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
    let (_project, compiled) = governance_project(
        r#"decision d1 "D" {
  status accepted
  context "c"
  decision "x"
  consequences "just a string"
}
"#,
    );

    let (code, diags) = check(&compiled);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert!(code);
    assert_eq!(
        exported_fields(&compiled, "d1")["consequences"],
        serde_json::json!(["just a string"])
    );
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a single reference on a reference_list field becomes a one-item list"
)]
fn a_single_reference_on_a_reference_list_field_links_like_a_list() {
    let (_project, compiled) = governance_project(
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

    let (code, diags) = check(&compiled);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert!(code);
    assert_eq!(
        exported_fields(&compiled, "d1")["invariants"],
        serde_json::json!(["inv1"])
    );

    // The single reference is an edge, exactly as `invariants [inv1]` is.
    let export = exported(&compiled);
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
    let (_project, compiled) = governance_project(&format!(
        r#"failure_mode fm1 "F" {{
  {FAILURE_MODE_FIELDS}
  rpn "abc"
}}
"#
    ));

    let (code, diags) = check(&compiled);
    assert!(!code, "{diags:?}");
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
    let (_project, compiled) = governance_project(&format!(
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

    let (code, diags) = check(&compiled);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert!(code);
    assert_eq!(
        exported_fields(&compiled, "fm1")["rpn"],
        serde_json::json!(12)
    );
    assert_eq!(
        exported_fields(&compiled, "fm2")["rpn"],
        serde_json::json!(40)
    );
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a list on a field declared as a single value is an error"
)]
fn a_list_on_a_string_field_is_an_error() {
    let (_project, compiled) = governance_project(
        r#"decision d1 "D" {
  status accepted
  context ["one", "two"]
  decision "x"
}
"#,
    );

    let (code, diags) = check(&compiled);
    assert!(!code, "{diags:?}");
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
    let (_project, compiled) = governance_project(
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

    let (code, diags) = check(&compiled);
    assert_eq!(diags, Vec::<serde_json::Value>::new());
    assert!(code);
    assert_eq!(
        exported_fields(&compiled, "d1")["consequences"],
        serde_json::json!(["5"])
    );
    assert_eq!(
        exported_fields(&compiled, "d2")["consequences"],
        serde_json::json!(["a", "7"])
    );
}

// -- the builtins' field vocabularies (plan 01) --

/// A project of the four builtins that declare vocabularies, over `spec`.
fn vocabulary_project(spec: &str) -> (TestProject, CompiledProject) {
    project_enabling(
        &[
            "@specforge/software",
            "@specforge/product",
            "@specforge/governance",
            "@specforge/formal",
        ],
        spec,
    )
}

/// One entity of every kind with a vocabulary, each field written with a
/// value outside its vocabulary (22 of them).
const OFF_VOCABULARY: &str = r#"feature feat "F" {
  problem  "p"
  status   finished
  priority urgent
  effort   huge
}

journey jour "J" {
  flow     ["a"]
  priority urgent
}

deliverable deli "D" {
  artifact_type binary
  status        released
}

milestone mile "M" {
  status   done
  priority urgent
}

persona pers "P" {
  description     "d"
  technical_level novice
  status          retired
}

channel chan "C" {
  description       "d"
  interaction_model sync
  status            retired
}

release rele "R" {
  version "1.0.0"
  status  shipped
}

invariant inva "I" {
  guarantee "g"
}

decision decn "Dc" {
  status   open
  context  "c"
  decision "x"
}

constraint cons "Co" {
  priority must
  metric   "a < 1"
  description "d"
}

failure_mode fail "Fm" {
  invariant       inva
  cause           "c"
  effect          "e"
  severity        8
  occurrence      often
  detection       never
  post_severity   9
  post_occurrence often
  post_detection  never
}

property prop "Pr" {
  expression    "always ok"
  property_type eventual
}
"#;

/// The `field/entity` of an E061 message: `field 'status' of feature 'f' is
/// declared enum (...), but was given ...` is `status/f`.
fn refused_field(message: &str) -> String {
    let mut quoted = message.split('\'').skip(1).step_by(2);
    let field = quoted.next().unwrap();
    let id = quoted.next().unwrap();
    format!("{field}/{id}")
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "a value outside a builtin vocabulary is E061"
)]
fn every_builtin_vocabulary_refuses_a_value_outside_it() {
    let (_project, compiled) = vocabulary_project(OFF_VOCABULARY);

    let (code, diags) = check(&compiled);
    assert!(!code, "{diags:?}");
    let mut refused: Vec<String> = diags
        .iter()
        .filter(|d| d["code"] == "E061")
        .map(|d| refused_field(d["message"].as_str().unwrap()))
        .collect();
    refused.sort();
    let mut expected: Vec<String> = [
        "status/feat",
        "priority/feat",
        "effort/feat",
        "priority/jour",
        "artifact_type/deli",
        "status/deli",
        "status/mile",
        "priority/mile",
        "technical_level/pers",
        "status/pers",
        "interaction_model/chan",
        "status/chan",
        "status/rele",
        "status/decn",
        "priority/cons",
        "severity/fail",
        "occurrence/fail",
        "detection/fail",
        "post_severity/fail",
        "post_occurrence/fail",
        "post_detection/fail",
        "property_type/prop",
    ]
    .map(String::from)
    .to_vec();
    expected.sort();
    assert_eq!(refused, expected, "{diags:?}");
    // The warnings these values drew are retired: the core says it once.
    let retired = [
        "W050", "W051", "W052", "W077", "W078", "W079", "W080", "W083", "W084", "W085", "W095",
        "W121", "W125",
    ];
    for d in &diags {
        let code = d["code"].as_str().unwrap();
        assert!(!retired.contains(&code), "{d}");
    }
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "an enum value suggests the closest declared value"
)]
fn an_off_vocabulary_value_suggests_the_closest_value() {
    let (_project, compiled) = project_enabling(
        &["@specforge/product"],
        "feature feat \"F\" {\n  problem \"p\"\n  status dne\n}\n",
    );

    let (code, diags) = check(&compiled);
    assert!(!code, "{diags:?}");
    let e061: Vec<_> = diags.iter().filter(|d| d["code"] == "E061").collect();
    assert_eq!(e061.len(), 1, "{diags:?}");
    assert_eq!(
        e061[0]["message"],
        "field 'status' of feature 'feat' is declared enum (proposed, accepted, in_progress, done, deferred, deprecated), but was given dne"
    );
    assert_eq!(e061[0]["suggestion"], "did you mean 'done'?");
}

/// The twelve closed filters of the product lists: (kind, field, values).
/// Written out here, not read from the extension: this is what the lists
/// take today and what `check` must keep admitting.
const CLOSED_FILTERS: [(&str, &str, &[&str]); 12] = [
    (
        "feature",
        "status",
        &[
            "proposed",
            "accepted",
            "in_progress",
            "done",
            "deferred",
            "deprecated",
        ],
    ),
    (
        "feature",
        "priority",
        &["critical", "high", "medium", "low"],
    ),
    (
        "journey",
        "priority",
        &["critical", "high", "medium", "low"],
    ),
    (
        "deliverable",
        "artifact_type",
        &[
            "cli",
            "service",
            "library",
            "web_app",
            "mobile_app",
            "api",
            "extension",
            "documentation",
            "package",
        ],
    ),
    (
        "deliverable",
        "status",
        &["draft", "in_progress", "shipped", "deprecated"],
    ),
    (
        "milestone",
        "status",
        &["planned", "in_progress", "completed", "blocked"],
    ),
    (
        "milestone",
        "priority",
        &["critical", "high", "medium", "low"],
    ),
    (
        "persona",
        "technical_level",
        &[
            "expert",
            "advanced",
            "intermediate",
            "beginner",
            "non_technical",
        ],
    ),
    ("persona", "status", &["active", "deprecated"]),
    (
        "channel",
        "interaction_model",
        &[
            "request_response",
            "event_driven",
            "batch",
            "streaming",
            "bidirectional",
            "manual",
        ],
    ),
    ("channel", "status", &["active", "deprecated"]),
    (
        "release",
        "status",
        &["planned", "in_progress", "released", "recalled"],
    ),
];

/// The plural a kind's list command goes by.
fn plural(kind: &str) -> String {
    format!("{kind}s")
}

/// An entity per (kind, closed field, value): `<kind>_<field>_<value>`,
/// writing that value, every other closed field of the kind its first one,
/// and the fields the kind requires.
fn every_closed_value() -> String {
    let mut spec = String::new();
    for (kind, field, values) in CLOSED_FILTERS {
        for value in values {
            spec.push_str(&format!("{kind} {kind}_{field}_{value} \"E\" {{\n"));
            spec.push_str(match kind {
                "feature" => "  problem \"p\"\n",
                "journey" => "  flow [\"a\"]\n",
                "persona" | "channel" => "  description \"d\"\n",
                "release" => "  version \"1.0.0\"\n",
                _ => "",
            });
            for (other_kind, other, other_values) in CLOSED_FILTERS {
                if other_kind != kind {
                    continue;
                }
                let written = if other == field {
                    value
                } else {
                    &other_values[0]
                };
                spec.push_str(&format!("  {other} {written}\n"));
            }
            if kind == "feature" {
                spec.push_str("  effort xs\n");
            }
            spec.push_str("}\n\n");
        }
    }
    spec
}

#[specforge_test(
    behavior = "validate_product_vocabularies",
    verify = "check admits every value the list filters take, and the lists find it"
)]
fn the_lists_take_every_value_check_admits() {
    let (_project, compiled) = vocabulary_project(&every_closed_value());

    let (_ok, diags) = check(&compiled);
    let refused: Vec<&serde_json::Value> = diags.iter().filter(|d| d["code"] == "E061").collect();
    assert!(refused.is_empty(), "{refused:?}");

    for (kind, field, values) in CLOSED_FILTERS {
        let last = values.last().unwrap();
        let (exit_code, listed) =
            command_document(&compiled, "product", &plural(kind), json!({ field: last }));
        assert_eq!(exit_code, 0, "{kind} --{field} {last}");
        let ids: Vec<&str> = listed[plural(kind)]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, [format!("{kind}_{field}_{last}")], "{kind} --{field}");
    }
}
