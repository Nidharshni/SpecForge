use crate::e2e_fixtures::*;
use assert_cmd::Command;
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

/// SOFTWARE_SPEC in a project that enables @specforge/software, so the
/// schema has registered kinds and edge types.
fn software_project() -> tempfile::TempDir {
    setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
        &[("main.spec", SOFTWARE_SPEC)],
    )
}

/// The kinds @specforge/software registers.
const SOFTWARE_KINDS: [&str; 5] = ["behavior", "event", "invariant", "port", "type"];

fn run_json(args: &[&str], dir: &std::path::Path) -> serde_json::Value {
    let output = specforge_cmd().args(args).arg(dir).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_json_stdout(&output)
}

fn strings(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not an array: {value}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn names(list: &serde_json::Value, key: &str) -> BTreeSet<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v[key].as_str().unwrap().to_string())
        .collect()
}

// --- Phase 1d: Schema command tests ---

#[specforge_test(
    behavior = "serve_schema_resource",
    verify = "specforge schema outputs full schema as JSON"
)]
fn schema_command_outputs_valid_json() {
    let dir = software_project();
    let parsed = run_json(&["schema"], dir.path());

    assert_eq!(
        parsed["schema_version"],
        serde_json::json!({"major": 1, "minor": 0, "patch": 0})
    );
    assert_eq!(
        names(&parsed["extensions"], "name"),
        strings(&serde_json::json!(["@specforge/software"]))
    );
    // Every kind the extension registers, with its field definitions.
    assert_eq!(
        names(&parsed["entity_kinds"], "name"),
        SOFTWARE_KINDS.iter().map(|s| s.to_string()).collect()
    );
    let behavior = parsed["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"] == "behavior")
        .unwrap();
    let contract = behavior["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "contract")
        .expect("behavior.contract");
    assert_eq!(contract["field_type"], "string");
    assert_eq!(contract["source_extension"], "@specforge/software");
    // And its edge types.
    let edge_types = names(&parsed["edge_types"], "label");
    for label in [
        "BehaviorEnforcesInvariant",
        "BehaviorProducesEvent",
        "BehaviorUsesPort",
    ] {
        assert!(
            edge_types.contains(label),
            "{label} missing: {edge_types:?}"
        );
    }
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "--kind with --publish, and --format without it, are refused"
)]
fn publish_refuses_kind_and_format_needs_publish() {
    let dir = software_project();
    let run = |args: &[&str]| specforge_cmd().args(args).arg(dir.path()).output().unwrap();

    let kind = run(&["schema", "--publish", "--kind", "behavior"]);
    assert_eq!(kind.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&kind.stderr);
    assert!(stderr.contains("--kind"), "{stderr}");

    let format = run(&["schema", "--format", "brief"]);
    assert_eq!(format.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&format.stderr);
    assert!(stderr.contains("--publish"), "{stderr}");

    // `json` is `graph`'s alias; plain `schema` runs with the default
    // `--format` (a default does not trigger `requires`).
    let json = run_json(&["schema", "--publish", "--format", "json"], dir.path());
    let graph = run_json(&["schema", "--publish", "--format", "graph"], dir.path());
    assert_eq!(json, graph);
    run_json(&["schema"], dir.path());
}

#[test]
fn schema_kind_filter_returns_single_kind() {
    // Note: with GraphProtocolSchema::empty(), entity_kinds is empty.
    // This test verifies the --kind flag behavior: unknown kind exits 1
    let dir = setup_project(&[("main.spec", SOFTWARE_SPEC)]);

    // Any kind filter on empty schema exits 1 (kind not found)
    specforge_cmd()
        .args(["schema", "--kind=behavior"])
        .arg(dir.path())
        .assert()
        .code(1);
}

#[test]
fn schema_kind_filter_unknown_exits_one() {
    let dir = setup_project(&[("main.spec", SOFTWARE_SPEC)]);

    specforge_cmd()
        .args(["schema", "--kind=nonexistent_kind"])
        .arg(dir.path())
        .assert()
        .code(1);
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema is valid JSON Schema"
)]
fn schema_publish_produces_json_schema_draft() {
    for dir in [
        software_project(),
        setup_project(&[("main.spec", SOFTWARE_SPEC)]),
    ] {
        let parsed = run_json(&["schema", "--publish"], dir.path());
        assert_eq!(
            parsed["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "published schema should have $schema field"
        );
        assert_eq!(parsed["title"], "SpecForge Graph Protocol");
        // Every keyword is a draft 2020-12 keyword with a well-typed value.
        let defs = parsed["$defs"]
            .as_object()
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default();
        assert_json_schema(&parsed, "#", &defs);
    }
}

/// Assert `node` is a well-formed JSON Schema (draft 2020-12) for the
/// keywords the published schema uses; `defs` are the `$defs` names.
fn assert_json_schema(node: &serde_json::Value, path: &str, defs: &BTreeSet<String>) {
    use serde_json::Value;
    const TYPES: [&str; 7] = [
        "object", "array", "string", "integer", "number", "boolean", "null",
    ];
    let obj = match node {
        Value::Bool(_) => return,
        Value::Object(obj) => obj,
        other => panic!("{path}: a schema must be an object or boolean, got {other}"),
    };
    for (key, value) in obj {
        let at = format!("{path}/{key}");
        match key.as_str() {
            "$schema" | "$id" | "title" | "description" | "$comment" | "pattern" | "format" => {
                assert!(value.is_string(), "{at} must be a string")
            }
            "type" => {
                let listed: Vec<&Value> = match value {
                    Value::Array(items) => items.iter().collect(),
                    single => vec![single],
                };
                for t in listed {
                    assert!(
                        t.as_str().is_some_and(|t| TYPES.contains(&t)),
                        "{at}: unknown type {t}"
                    );
                }
            }
            "properties" | "$defs" => {
                for (name, sub) in value.as_object().unwrap_or_else(|| panic!("{at}: object")) {
                    assert_json_schema(sub, &format!("{at}/{name}"), defs);
                }
            }
            "items"
            | "additionalProperties"
            | "if"
            | "then"
            | "else"
            | "not"
            | "contains"
            | "propertyNames" => assert_json_schema(value, &at, defs),
            "allOf" | "anyOf" | "oneOf" => {
                let items = value.as_array().unwrap_or_else(|| panic!("{at}: array"));
                assert!(!items.is_empty(), "{at}: empty");
                for (i, sub) in items.iter().enumerate() {
                    assert_json_schema(sub, &format!("{at}/{i}"), defs);
                }
            }
            "minItems" | "maxItems" | "minLength" | "maxLength" | "minProperties" => {
                assert!(value.is_u64(), "{at} must be a non-negative integer")
            }
            "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
                assert!(value.is_number(), "{at} must be a number")
            }
            "uniqueItems" => assert!(value.is_boolean(), "{at} must be a boolean"),
            "examples" => assert!(value.is_array(), "{at} must be an array"),
            "required" => {
                // Inside a sub-schema (then/allOf) `required` may name
                // properties declared by the enclosing schema.
                let props = obj.get("properties").and_then(Value::as_object);
                for name in value.as_array().unwrap_or_else(|| panic!("{at}: array")) {
                    let name = name.as_str().unwrap_or_else(|| panic!("{at}: strings"));
                    if let Some(props) = props {
                        assert!(
                            props.contains_key(name),
                            "{at}: required '{name}' is not a declared property"
                        );
                    }
                }
            }
            "enum" => {
                let items = value.as_array().unwrap_or_else(|| panic!("{at}: array"));
                assert!(!items.is_empty(), "{at}: empty enum");
                let unique: BTreeSet<String> = items.iter().map(|v| v.to_string()).collect();
                assert_eq!(unique.len(), items.len(), "{at}: duplicate enum values");
            }
            "const" => {}
            "$ref" => {
                let target = value.as_str().unwrap_or_default();
                let name = target
                    .strip_prefix("#/$defs/")
                    .unwrap_or_else(|| panic!("{at}: unsupported $ref {target}"));
                assert!(defs.contains(name), "{at}: $ref to missing {target}");
            }
            other => panic!("{at}: unexpected keyword '{other}'"),
        }
    }
}

/// Validate `instance` against `schema` (draft 2020-12), for the keywords
/// `assert_json_schema` admits. Returns one message per violation. A keyword
/// the validator does not implement panics, so a schema can never pass
/// by using something unchecked.
fn validate(
    instance: &serde_json::Value,
    schema: &serde_json::Value,
    root: &serde_json::Value,
    at: &str,
) -> Vec<String> {
    use serde_json::Value;
    let obj = match schema {
        Value::Bool(true) => return vec![],
        Value::Bool(false) => return vec![format!("{at}: rejected by `false` schema")],
        Value::Object(obj) => obj,
        other => panic!("{at}: a schema must be an object or boolean, got {other}"),
    };
    let type_of = |v: &Value| match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let mut errors = Vec::new();
    for (key, value) in obj {
        match key.as_str() {
            // Annotations: they never fail validation.
            "$schema" | "$id" | "title" | "description" | "$comment" | "format" | "examples"
            | "$defs" | "then" | "else" => {}
            "type" => {
                let actual = type_of(instance);
                let allowed: Vec<&str> = match value {
                    Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
                    single => vec![single.as_str().unwrap()],
                };
                let ok = allowed
                    .iter()
                    .any(|t| *t == actual || (*t == "number" && actual == "integer"));
                if !ok {
                    errors.push(format!("{at}: {actual} is not {allowed:?}"));
                }
            }
            "minimum" => {
                if instance
                    .as_f64()
                    .is_some_and(|n| n < value.as_f64().unwrap())
                {
                    errors.push(format!("{at}: {instance} is below {value}"));
                }
            }
            "enum" => {
                if !value.as_array().unwrap().contains(instance) {
                    errors.push(format!("{at}: {instance} is not one of {value}"));
                }
            }
            "const" => {
                if instance != value {
                    errors.push(format!("{at}: {instance} is not {value}"));
                }
            }
            "required" => {
                if let Some(o) = instance.as_object() {
                    for name in value.as_array().unwrap() {
                        let name = name.as_str().unwrap();
                        if !o.contains_key(name) {
                            errors.push(format!("{at}: missing required '{name}'"));
                        }
                    }
                }
            }
            "properties" => {
                if let Some(o) = instance.as_object() {
                    for (name, sub) in value.as_object().unwrap() {
                        if let Some(v) = o.get(name) {
                            errors.extend(validate(v, sub, root, &format!("{at}/{name}")));
                        }
                    }
                }
            }
            "additionalProperties" => {
                if let Some(o) = instance.as_object() {
                    let declared = obj.get("properties").and_then(Value::as_object);
                    for (name, v) in o {
                        if !declared.is_some_and(|d| d.contains_key(name)) {
                            errors.extend(validate(v, value, root, &format!("{at}/{name}")));
                        }
                    }
                }
            }
            "items" => {
                if let Some(items) = instance.as_array() {
                    for (i, v) in items.iter().enumerate() {
                        errors.extend(validate(v, value, root, &format!("{at}/{i}")));
                    }
                }
            }
            "allOf" => {
                for sub in value.as_array().unwrap() {
                    errors.extend(validate(instance, sub, root, at));
                }
            }
            "anyOf" => {
                let any = value
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|sub| validate(instance, sub, root, at).is_empty());
                if !any {
                    errors.push(format!("{at}: matches no anyOf branch"));
                }
            }
            "if" => {
                let branch = if validate(instance, value, root, at).is_empty() {
                    obj.get("then")
                } else {
                    obj.get("else")
                };
                if let Some(branch) = branch {
                    errors.extend(validate(instance, branch, root, at));
                }
            }
            "$ref" => {
                let name = value.as_str().unwrap().strip_prefix("#/$defs/").unwrap();
                errors.extend(validate(instance, &root["$defs"][name], root, at));
            }
            other => panic!("{at}: the validator does not implement '{other}'"),
        }
    }
    errors
}

/// A project with an edge from nearly every reference field the software,
/// product and governance extensions declare, plus a `refs` list whose
/// entries name entities.
const EDGES_SPEC: &str = r#"
invariant graph_acyclic "Graph Acyclicity" {
  guarantee "The dependency graph MUST be acyclic"
}

type parse_result "Parse Result" {
  description "Result of a parse"
}

type typed_result "Typed Result" {
  description    "A parse result with types"
  extends        parse_result
  composed_types [parse_result]
}

port file_reader "File Reader" {
  direction inbound
}

event input_parsed "Input Parsed" {
  payload parse_result
}

behavior parse_input "Parse Input" {
  contract   "The system MUST parse all valid input"
  invariants [graph_acyclic]
  types      [parse_result]
  ports      [file_reader]
  produces   [input_parsed]
  features   [fast_parsing]
  refs       [emit_output]
}

behavior emit_output "Emit Output" {
  contract "The system MUST emit structured output"
  consumes [input_parsed]
}

feature fast_parsing "Fast Parsing" {
  problem    "Users need quick feedback"
  solution   "Incremental parsing"
  depends_on [slow_parsing]
}

feature slow_parsing "Slow Parsing" {
  problem  "Some input is large"
  solution "Batch parsing"
}

persona developer "Developer" {
  description  "Writes specs"
  key_features [fast_parsing]
}

journey developer_workflow "Developer Workflow" {
  description "A developer writes and validates specs"
  flow        ["write a spec", "run check"]
  persona     developer
  features    [fast_parsing]
}

module parser_module "Parser Module" {
  description   "Handles .spec file parsing"
  features      [fast_parsing]
  ports_defined [file_reader]
}

milestone v1_release "V1 Release" {
  description "First stable release"
  behaviors   [parse_input]
  features    [fast_parsing]
  modules     [parser_module]
}

deliverable cli_tool "CLI Tool" {
  description   "The specforge CLI binary"
  artifact_type "cli"
  journeys      [developer_workflow]
  modules       [parser_module]
  milestones    [v1_release]
}

term spec_file "Spec File" {
  definition "A .spec file"
  module     parser_module
}

decision use_treesitter "Use Tree-Sitter" {
  status           accepted
  context          "We need a parser"
  decision         "Use tree-sitter"
  consequences     ["Fast incremental parsing"]
  invariants       [graph_acyclic]
  affects_features [fast_parsing]
}

constraint parse_latency "Parse Latency" {
  category    performance
  priority    critical
  description "Parsing MUST finish in 100ms"
  constrains  [parse_input]
  protects    [graph_acyclic]
}

failure_mode parser_crash "Parser Crash" {
  description        "The parser panics on bad input"
  cause              "malformed input"
  effect             "no output"
  severity           "high"
  affected_behaviors [parse_input]
  invariant          graph_acyclic
  threatens_features [fast_parsing]
}
"#;

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema validates known-good export"
)]
fn schema_publish_validates_a_real_export() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/product","@specforge/governance"]}"#,
        &[("main.spec", EDGES_SPEC)],
    );
    let published = run_json(&["schema", "--publish"], dir.path());
    let export = run_json(&["export", "--format=graph"], dir.path());

    // The export carries an edge for every reference field the spec fills,
    // each labelled with the field's name.
    let labels = names(&export["edges"], "label");
    assert_eq!(
        labels,
        strings(&serde_json::json!([
            "affected_behaviors",
            "affects_features",
            "behaviors",
            "composed_types",
            "constrains",
            "consumes",
            "depends_on",
            "extends",
            "features",
            "invariant",
            "invariants",
            "journeys",
            "key_features",
            "milestones",
            "module",
            "modules",
            "payload",
            "persona",
            "ports",
            "ports_defined",
            "produces",
            "protects",
            "refs",
            "threatens_features",
            "types"
        ]))
    );

    let errors = validate(&export, &published, &published, "#");
    assert!(
        errors.is_empty(),
        "the export does not validate against the published schema:\n{}",
        errors.join("\n")
    );

    // The validator does reject what the schema forbids.
    let mut broken = export.clone();
    broken["edges"][0]["label"] = serde_json::json!(7);
    broken["nodes"][0]["kind"] = serde_json::json!("no_such_kind");
    let mut rejected = validate(&broken, &published, &published, "#");
    rejected.sort();
    assert_eq!(
        rejected,
        vec![
            "#/edges/0/label: integer is not [\"string\"]".to_string(),
            format!(
                "#/nodes/0/kind: \"no_such_kind\" is not one of {}",
                published["properties"]["nodes"]["items"]["properties"]["kind"]["enum"]
            ),
        ]
    );
}

#[specforge_test(
    behavior = "check_field_value_types",
    verify = "an export with a coerced string_list validates against the published schema"
)]
fn schema_publish_validates_an_export_with_a_coerced_string_list() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/governance"]}"#,
        &[(
            "main.spec",
            r#"decision d1 "D" {
  status accepted
  context "c"
  decision "x"
  consequences "just a string"
}
failure_mode fm1 "F" {
  cause "disk full"
  effect "writes fail"
  severity high
  rpn "12"
}
"#,
        )],
    );
    let published = run_json(&["schema", "--publish"], dir.path());
    let export = run_json(&["export", "--format=graph"], dir.path());

    let errors = validate(&export, &published, &published, "#");
    assert!(
        errors.is_empty(),
        "the export does not validate against the published schema:\n{}",
        errors.join("\n")
    );
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema describes all registered entity kinds"
)]
fn schema_publish_includes_node_kind_enum() {
    let dir = software_project();
    let parsed = run_json(&["schema", "--publish"], dir.path());

    let kind = &parsed["properties"]["nodes"]["items"]["properties"]["kind"];
    assert_eq!(kind["type"], "string");
    assert_eq!(
        strings(&kind["enum"]),
        SOFTWARE_KINDS.iter().map(|s| s.to_string()).collect(),
        "the node kind enum lists every registered kind"
    );
    // The same set `specforge schema` reports as registered.
    let registered = run_json(&["schema"], dir.path());
    assert_eq!(
        strings(&kind["enum"]),
        names(&registered["entity_kinds"], "name")
    );
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "published schema describes all edge types"
)]
fn schema_publish_includes_edge_label_enum() {
    let dir = software_project();
    let parsed = run_json(&["schema", "--publish"], dir.path());

    // The embedded schema block names every registered edge type, no more.
    let edge_type = &parsed["properties"]["schema"]["properties"]["edge_types"]["items"];
    let label = &edge_type["properties"]["label"];
    assert_eq!(label["type"], "string");
    let listed = strings(&label["enum"]);
    let registered = run_json(&["schema"], dir.path());
    assert_eq!(listed, names(&registered["edge_types"], "label"));
    for expected in [
        "BehaviorEnforcesInvariant",
        "BehaviorProducesEvent",
        "BehaviorConsumesEvent",
        "BehaviorUsesPort",
        "EventCarriesPayloadType",
    ] {
        assert!(listed.contains(expected), "{expected} missing: {listed:?}");
    }

    // A graph edge is labelled by the field that declared it. The label is
    // open, and the fields that declare an edge type are its examples.
    let edge_label = &parsed["properties"]["edges"]["items"]["properties"]["label"];
    assert_eq!(edge_label["type"], "string");
    assert!(edge_label.get("enum").is_none(), "{edge_label}");
    let declaring: BTreeSet<String> = registered["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|k| k["fields"].as_array().unwrap())
        .filter(|f| f["edge"].is_string())
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        declaring,
        strings(&serde_json::json!([
            "composed_types",
            "consumes",
            "extends",
            "features",
            "invariants",
            "payload",
            "ports",
            "produces",
            "types"
        ]))
    );
    assert_eq!(strings(&edge_label["examples"]), declaring);
}

#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "schema embedded as top-level key in full JSON export"
)]
fn export_v2_schema_has_entity_kinds() {
    let dir = setup_project(&[("main.spec", SOFTWARE_SPEC)]);

    let output = specforge_cmd()
        .args(["export", "--format=graph"])
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    assert_eq!(parsed["format_version"], "2.0");
    assert!(
        parsed["schema"]["entity_kinds"].is_array(),
        "V2 schema should have entity_kinds"
    );
}

#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "format_version set to 2.0 with schema"
)]
fn export_v2_schema_has_edge_types() {
    let dir = software_project();
    let parsed = run_json(&["export", "--format=graph"], dir.path());

    // Format 2.0, carrying the project's schema.
    assert_eq!(parsed["format_version"], "2.0");
    let registered = run_json(&["schema"], dir.path());
    assert_eq!(
        parsed["schema"], registered,
        "the embedded schema is the project's"
    );
    assert!(
        names(&parsed["schema"]["edge_types"], "label").contains("BehaviorEnforcesInvariant"),
        "{}",
        parsed["schema"]["edge_types"]
    );

    // Without the schema, the export is not 2.0.
    let bare = run_json(&["export", "--format=graph", "--no-schema"], dir.path());
    assert!(bare.get("schema").is_none());
    assert_ne!(bare["format_version"], "2.0");
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "compatible version within range is resolved"
)]
fn export_schema_version_negotiation() {
    let dir = setup_project(&[("main.spec", SOFTWARE_SPEC)]);

    // Valid version (1.0.0 matches the current schema version)
    let output = specforge_cmd()
        .args(["export", "--format=graph", "--schema-version=1.0.0"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "1.0.0 should be accepted");

    // Invalid major version
    specforge_cmd()
        .args(["export", "--format=graph", "--schema-version=99.0.0"])
        .arg(dir.path())
        .assert()
        .code(1);
}

#[specforge_test(
    behavior = "embed_schema_in_export",
    verify = "scoped exports carry schema_ref (url and content_hash) instead of embedded schema"
)]
fn export_scoped_v2_references_schema() {
    let dir = setup_project(&[(
        "main.spec",
        r#"
behavior alpha "A" { contract "first" }
feature gamma "G" { behaviors [alpha] }
"#,
    )]);

    let output = specforge_cmd()
        .args(["export", "--format=graph", "--scope=alpha"])
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed = parse_json_stdout(&output);
    assert_eq!(parsed["format_version"], "2.0");
    assert!(
        parsed.get("schema").is_none(),
        "scoped V2 must not embed the full schema"
    );
    assert!(
        parsed["schema_ref"]["url"].is_string(),
        "scoped V2 carries a schema_ref url"
    );
    assert_eq!(
        parsed["schema_ref"]["content_hash"].as_str().unwrap().len(),
        64,
        "schema_ref content_hash is a sha256 hex digest"
    );
}

#[test]
fn schema_publish_describes_the_requested_format() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("specforge.json"),
        r#"{"name":"s","spec_root":"src","extensions":["@specforge/product"]}"#,
    )
    .unwrap();
    std::fs::write(root.join("src/a.spec"), "type W { id string @unique }").unwrap();

    let run = |fmt: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_specforge"))
            .args([
                "schema",
                root.to_str().unwrap(),
                "--publish",
                "--format",
                fmt,
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{fmt}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("published schema is JSON");
        v["properties"]["nodes"]["items"].clone()
    };

    let full = run("graph");
    assert_eq!(
        full["required"],
        serde_json::json!(["id", "kind", "file", "line", "fields"]),
        "graph schema describes full nodes"
    );

    let context = run("context");
    assert_eq!(
        context["required"],
        serde_json::json!(["id", "kind"]),
        "context schema describes context nodes (no file/line/fields required)"
    );
    assert!(
        context["properties"].get("verify").is_some(),
        "context schema knows the verify field"
    );
    assert!(context["properties"].get("file").is_none());

    let brief = run("brief");
    assert_eq!(
        brief["properties"]
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["id".into(), "kind".into(), "title".into()]),
        "brief schema exposes exactly id/kind/title"
    );
}

// --- Exports validate against what is published (ADR-NEW-01, plan 01) ---

/// The messages for which `export` does not conform to `published`: the
/// published schema's own check, one message per violation.
fn conforms(published: &serde_json::Value, export: &serde_json::Value) -> Vec<String> {
    validate(export, published, published, "#")
}

/// Copy `<repo>/<rel>` into a tempdir, leaving out `.specforge` and
/// `target`; for the repository itself, only `specforge.json` and `spec/`.
/// `export` records the schema cache in the project it exports, so a test
/// never exports a committed directory.
fn copy_project(rel: &str) -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == ".specforge" || name == "target" {
                continue;
            }
            let (from, to) = (entry.path(), to.join(&name));
            if entry.file_type().unwrap().is_dir() {
                copy(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = tempfile::TempDir::new().unwrap();
    if rel == "." {
        std::fs::copy(
            repo.join("specforge.json"),
            tmp.path().join("specforge.json"),
        )
        .unwrap();
        copy(&repo.join("spec"), &tmp.path().join("spec"));
    } else {
        copy(&repo.join(rel), tmp.path());
    }
    tmp
}

/// The projects whose exports must validate against what they publish.
const PUBLISHED_PROJECTS: [&str; 9] = [
    ".",
    "integrations/rust/spec",
    "examples/todo-app",
    "examples/shop",
    "crates/specforge-cli/tests/fixtures/parity/clean",
    "crates/specforge-cli/tests/fixtures/parity/body_parser_type",
    "crates/specforge-cli/tests/fixtures/read_views/rv1",
    "crates/specforge-cli/tests/fixtures/coverage/fx1",
    "xtask/tests/fixtures/links",
];

/// Every named format's export of `dir` against the format's published
/// schema; one message per violation, prefixed by the format.
fn export_violations(dir: &std::path::Path, formats: &[&str]) -> Vec<String> {
    let mut messages = Vec::new();
    for format in formats {
        let published = run_json(&["schema", "--publish", "--format", format], dir);
        let export = run_json(&["export", "--format", format], dir);
        messages.extend(
            conforms(&published, &export)
                .into_iter()
                .map(|m| format!("{format}: {m}")),
        );
    }
    messages
}

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "a context or brief export validates against the published schema of its format"
)]
#[ignore = "fixed by 01-T6"]
fn agent_exports_validate_against_their_published_schema() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#,
        &[(
            "main.spec",
            "spec \"fixture\" {\n  version \"1.0\"\n}\n\nbehavior register \"Register\" {\n  contract \"registers an id\"\n}\n",
        )],
    );
    for with_schema in [false, true] {
        for format in ["context", "brief"] {
            let published = run_json(&["schema", "--publish", "--format", format], dir.path());
            let mut args = vec!["export", "--format", format];
            if with_schema {
                args.push("--with-schema");
            }
            let export = run_json(&args, dir.path());
            let messages = conforms(&published, &export);
            assert!(
                messages.is_empty(),
                "{format} (with_schema {with_schema}):\n{}",
                messages.join("\n")
            );
        }
    }
}

const ADMITTED_SPEC: &str = r#"spec "fixture" {
  version "1.0"
}

invariant unique_ids "Unique ids" {
  guarantee "ids are unique"
  expression expr {
    duplicates < 1
    collisions <= 0
  }
}

behavior register "Register" {
  contract   "registers an id"
  invariants [unique_ids]
  produces   [registered]
  requires {
    id_free "the id is not taken"
  }
  ensures {
    id_taken "the id is taken"
  }
  maintains {
    unique_ids "ids stay unique"
  }
}

type Registration {
  id     string
  fields RegistrationFields
  kind   string
}

type RegistrationFields {
  entries string[]
}

event registered "Registered" {
  payload Registration
  sync {
    timeout "5s"
  }
}

failure_mode clash "Clash" {
  cause    "two writers"
  effect   "duplicate ids"
  severity "high"
  rpn      12
  post_mitigation {
    severity low
    rpn      2
  }
}

decision d1 "D" {
  status       accepted
  context      "c"
  decision     "x"
  consequences 5
}
"#;

#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "an export of a project with no errors validates against the published schema"
)]
#[ignore = "fixed by 01-T8"]
fn an_export_of_a_project_check_admits_validates_against_its_published_schema() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/product","@specforge/governance","@specforge/formal"]}"#,
        &[("main.spec", ADMITTED_SPEC)],
    );
    specforge_cmd()
        .args(["check"])
        .arg(dir.path())
        .assert()
        .success();
    let messages = export_violations(dir.path(), &["graph", "context", "brief"]);
    assert!(messages.is_empty(), "{}", messages.join("\n"));
}

#[specforge_test(
    behavior = "export_agent_graph_format",
    verify = "output conforms to Graph Protocol schema"
)]
#[specforge_test(
    behavior = "publish_schema_specification",
    verify = "this repository's and its example projects' exports validate against their published schemas"
)]
#[ignore = "fixed by 01-T8"]
fn this_repository_and_its_example_projects_export_what_they_publish() {
    for rel in PUBLISHED_PROJECTS {
        let dir = copy_project(rel);
        specforge_cmd()
            .args(["check"])
            .arg(dir.path())
            .assert()
            .success();
        let messages = export_violations(dir.path(), &["graph", "context", "brief"]);
        assert!(messages.is_empty(), "{rel}:\n{}", messages.join("\n"));
    }
}
