//! Extension validation rules through a real load and compile (the rule
//! set, ADR 0020). Each test declares its rules raw (`raw_category`), so
//! rules the SDK's builders would refuse can be written, on the kind `node`
//! of an in-process extension. A pin that still encodes a bug says which
//! ticket of plan 02 flips it.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_project::CompiledProject;
use specforge_test_macros::test as spec;
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;
use tempfile::TempDir;

const EXT: &str = "@pin/rules";

/// The extension's kind `node` (testable, accepts `verify`): `deps`
/// (reference list writing the edge `NodeDependsOn` to `node`), `doc`,
/// `status` and `reason` (strings), and `owner`, a string that is
/// `required` when `required_owner` is set. `rules` is its
/// `validation_rules` as written.
fn extension(rules: Value, required_owner: bool) -> impl Fn() -> ContributionsBuilder {
    move || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "0.1.0"));
        c.kind("node", |k| {
            k.description("A node").testable(true).supports_verify(true);
            k.field("deps", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("NodeDependsOn")
                    .target_kind("node");
            });
            for field in ["doc", "status", "reason"] {
                k.field(field, |f| {
                    f.field_type(FieldType::String);
                });
            }
            k.field("owner", |f| {
                f.field_type(FieldType::String);
                if required_owner {
                    f.required();
                }
            });
        });
        c.edge("NodeDependsOn", |e| {
            e.source_kind("node").target_kind("node");
        });
        c.raw_category(
            specforge_extension_sdk::DeclaredCategory::ValidationRules,
            rules.clone(),
        );
        c
    }
}

/// `validate__x007`: fails `gamma`, cannot answer for `beta` (the way a
/// trap reaches the host: the call errs), passes every other entity.
fn x007(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    if export != "validate__x007" {
        return None;
    }
    let context: ValidatorContext = match serde_json::from_slice(input) {
        Ok(context) => context,
        Err(e) => return Some(Err(e.to_string())),
    };
    if context.entity.id == "beta" {
        return Some(Err("cannot read beta".to_string()));
    }
    Some(specforge_extension_sdk::answer_export(
        export,
        input,
        |context: &ValidatorContext| {
            if context.entity.id == "gamma" {
                ValidatorVerdict::Fail {
                    field: None,
                    value: None,
                }
            } else {
                ValidatorVerdict::Pass
            }
        },
    ))
}

/// The three nodes of the plan's reproduction: `alpha` and `beta` depend
/// on each other, `gamma` names a file only the spec root holds.
const NODES: &str = r#"node alpha "Alpha" {
  deps [beta]
}

node beta "Beta" {
  deps [alpha]
}

node gamma "Gamma" {
  doc "pin-only-under-spec-root.md"
}
"#;

/// A project (`spec_root: "spec"`) whose `spec/nodes.spec` is `source`,
/// with `spec/pin-only-under-spec-root.md` present.
fn project(source: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config =
        json!({ "name": "pins", "version": "0.1.0", "spec_root": "spec", "extensions": [EXT] });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::create_dir(dir.path().join("spec")).unwrap();
    fs::write(dir.path().join("spec/nodes.spec"), source).unwrap();
    fs::write(
        dir.path().join("spec/pin-only-under-spec-root.md"),
        "# pin\n",
    )
    .unwrap();
    dir
}

/// The compile of `root` with the extension declaring `rules`.
fn compile_with(root: &Path, rules: Value, required_owner: bool) -> CompiledProject {
    let runtime = InProcessRuntime::new().with_handler(extension(rules, required_owner), x007);
    CompiledProject::compile(
        &specforge_common::ProjectRoot::of(root).unwrap(),
        Some(Arc::new(runtime)),
    )
}

/// The diagnostics of compiling `source` with the extension declaring `rules`.
fn check(source: &str, rules: Value) -> Vec<Diagnostic> {
    let dir = project(source);
    compile_with(dir.path(), rules, false).diagnostics()
}

/// The messages of the diagnostics with `code`.
fn messages<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.as_str())
        .collect()
}

/// The diagnostics with `code` that mention `part`.
fn naming<'a>(diagnostics: &'a [Diagnostic], code: &str, part: &str) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.code == code && d.message.contains(part))
        .collect()
}

/// A cycle rule `code` on `NodeDependsOn` (the control of P1).
fn cycle(code: &str) -> Value {
    json!({
        "code": code, "severity": "error", "check": "cycle_detection",
        "message_template": format!("{code} cycle through '{{id}}'"),
        "target_kind": "node", "edge_type": "NodeDependsOn"
    })
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a cycle_detection rule without an edge_type produces W112 and is not registered"
)]
fn a_cycle_rule_without_an_edge_type_is_w112() {
    let diagnostics = check(
        NODES,
        json!([
            cycle("X001"),
            {
                "code": "X002", "severity": "error", "check": "cycle_detection",
                "message_template": "X002 cycle through '{id}'", "target_kind": "node"
            }
        ]),
    );

    assert_eq!(
        messages(&diagnostics, "X001"),
        ["X001 cycle through 'alpha'", "X001 cycle through 'beta'"]
    );
    assert!(messages(&diagnostics, "X002").is_empty(), "{diagnostics:?}");
    assert_eq!(
        messages(&diagnostics, "W112"),
        [
            "extension '@pin/rules': rule 'X002': check 'cycle_detection' requires an edge_type but none is set — the rule can never fire and was not registered"
        ]
    );
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "a cycle_detection rule without a target_kind reports every entity on a cycle of its edge type"
)]
fn a_cycle_rule_without_a_target_kind_checks_every_entity() {
    let diagnostics = check(
        NODES,
        json!([{
            "code": "X008", "severity": "error", "check": "cycle_detection",
            "message_template": "X008 cycle through '{id}'", "edge_type": "NodeDependsOn"
        }]),
    );

    assert_eq!(
        messages(&diagnostics, "X008"),
        ["X008 cycle through 'alpha'", "X008 cycle through 'beta'"]
    );
}

#[spec(
    behavior = "registry_build_rules",
    verify = "a rule whose edge type no loaded extension declares reports nothing"
)]
fn an_edge_rule_naming_an_undeclared_edge_type_reports_nothing() {
    let diagnostics = check(
        NODES,
        json!([{
            "code": "X005", "severity": "info", "check": "no_outgoing_edges",
            "message_template": "X005 '{id}' has no NoSuchEdge edge",
            "target_kind": "node", "edge_type": "NoSuchEdge"
        }]),
    );

    // Not even gamma, which has no edge at all: the rule is inert.
    assert!(messages(&diagnostics, "X005").is_empty(), "{diagnostics:?}");
    assert_eq!(naming(&diagnostics, "W021", "'X005'").len(), 1);
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a rule's edge type that neither its extension nor its peers declare produces W021"
)]
fn a_cycle_rule_naming_an_undeclared_edge_type_is_inert_and_w021() {
    let diagnostics = check(
        NODES,
        json!([{
            "code": "X003", "severity": "error", "check": "cycle_detection",
            "message_template": "X003 cycle through '{id}'",
            "target_kind": "node", "edge_type": "NoSuchEdge"
        }]),
    );

    assert!(messages(&diagnostics, "X003").is_empty(), "{diagnostics:?}");
    assert_eq!(
        messages(&diagnostics, "W021"),
        [
            "extension '@pin/rules': rule 'X003' references edge type 'NoSuchEdge' not declared among its edges or its peers' edges"
        ]
    );
}

// REGRESSION (01-T5): file_exists resolves against the spec root, never the
// working directory. Never flipped.
#[test]
fn file_exists_reads_the_spec_root_whatever_the_working_directory() {
    let rules = json!([{
        "code": "X004", "severity": "warning", "check": "file_exists",
        "message_template": "X004 '{id}': missing file '{value}'",
        "target_kind": "node", "field": "doc"
    }]);
    let dir = project(NODES);
    // The test's working directory does not hold the file.
    assert!(!Path::new("pin-only-under-spec-root.md").exists());

    let diagnostics = compile_with(dir.path(), rules.clone(), false).diagnostics();
    assert!(messages(&diagnostics, "X004").is_empty(), "{diagnostics:?}");

    fs::remove_file(dir.path().join("spec/pin-only-under-spec-root.md")).unwrap();
    let diagnostics = compile_with(dir.path(), rules, false).diagnostics();
    assert_eq!(
        messages(&diagnostics, "X004"),
        ["X004 'gamma': missing file 'pin-only-under-spec-root.md'"]
    );
}

#[spec(
    behavior = "register_custom_validation_patterns",
    verify = "a custom rule whose function fails on entities produces one W148 per check naming how many were not checked"
)]
fn a_custom_function_failing_on_an_entity_is_w148() {
    let diagnostics = check(
        NODES,
        json!([{
            "code": "X007", "severity": "warning", "check": "custom",
            "message_template": "X007 '{id}' failed the custom check",
            "target_kind": "node", "wasm_function": "validate__x007"
        }]),
    );

    assert_eq!(
        messages(&diagnostics, "X007"),
        ["X007 'gamma' failed the custom check"]
    );
    assert!(
        naming(&diagnostics, "W112", "X007").is_empty(),
        "{diagnostics:?}"
    );
    // beta, which the function cannot answer for, is reported once.
    let w148: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == "W148").collect();
    assert_eq!(w148.len(), 1, "{diagnostics:?}");
    let message = &w148[0].message;
    assert!(
        message.starts_with(
            "extension '@pin/rules': rule 'X007': wasm_function 'validate__x007' failed on 1 entity ('beta': "
        ) && message.contains("cannot read beta")
            && message.ends_with(") — it was not checked"),
        "{message}"
    );
    // `check --format json` carries every failure under `data`.
    let json = serde_json::to_value(specforge_common::diagnostics_json(&diagnostics)).unwrap();
    let entry = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "W148")
        .unwrap();
    assert_eq!(entry["data"]["kind"], "custom_rule_failures");
    assert_eq!(entry["data"]["rule"], "X007");
    assert_eq!(entry["data"]["function"], "validate__x007");
    assert_eq!(entry["data"]["failures"][0]["entity"], "beta");
    assert_eq!(entry["data"]["failures"].as_array().unwrap().len(), 1);
}

// Stable: declared rules in code order, then the host's E006 rules; each
// rule's entities in id order. Never flipped.
#[test]
fn rule_diagnostics_come_in_rule_order_then_entity_id_order() {
    let dir = project(
        "node gamma \"Gamma\" {\n}\n\nnode alpha \"Alpha\" {\n  deps [beta]\n}\n\n\
         node beta \"Beta\" {\n  deps [alpha]\n}\n",
    );
    let rules = json!([
        cycle("X001"),
        {
            "code": "W900", "severity": "warning", "check": "no_incoming_edges",
            "message_template": "W900 '{id}' is unreferenced", "target_kind": "node"
        },
        {
            "code": "E901", "severity": "error", "check": "missing_required_field",
            "message_template": "E901 '{id}' has no reason", "target_kind": "node",
            "field": "reason"
        }
    ]);

    let diagnostics = compile_with(dir.path(), rules, true).diagnostics();

    let order: Vec<String> = diagnostics
        .iter()
        .filter(|d| ["E901", "W900", "X001", "E006"].contains(&d.code.as_str()))
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert_eq!(
        order,
        [
            "E901 E901 'alpha' has no reason",
            "E901 E901 'beta' has no reason",
            "E901 E901 'gamma' has no reason",
            "W900 W900 'gamma' is unreferenced",
            "X001 X001 cycle through 'alpha'",
            "X001 X001 cycle through 'beta'",
            "E006 node 'alpha' is missing required field 'owner'",
            "E006 node 'beta' is missing required field 'owner'",
            "E006 node 'gamma' is missing required field 'owner'",
        ]
    );
}

// REGRESSION (01-T3): a rule without a target kind obliges every kind, for
// the rule, the standing and the pass input alike. Never flipped.
#[test]
fn an_untargeted_obligation_rule_obliges_every_kind_after_the_move() {
    let dir = project("node alpha \"Alpha\" {\n}\n");
    let rules = json!([{
        "code": "W904", "severity": "warning", "check": "no_verify_statements",
        "message_template": "{kind} '{id}' declares no verify obligations"
    }]);

    let compiled = compile_with(dir.path(), rules, false);

    assert_eq!(
        messages(&compiled.diagnostics(), "W904"),
        ["node 'alpha' declares no verify obligations"]
    );
    let standing = compiled.entities().standing("alpha").unwrap();
    assert_eq!(standing.rule.as_deref(), Some("W904"));
    assert!(standing.owes_obligations());
    let alpha = compiled
        .entities()
        .pass_entities()
        .into_iter()
        .find(|e| e.id == "alpha")
        .unwrap();
    assert!(!alpha.owes_none);
}

#[spec(
    behavior = "parse_validation_rule_pattern",
    verify = "a conditional_field_required constraint of another kind produces W147 and is read as when_field_equals"
)]
fn a_conditional_rule_with_a_matches_constraint_is_w147_and_reads_pattern_as_a_field() {
    let diagnostics = check(
        "node alpha \"Alpha\" {\n  status \"deferred\"\n}\n\n\
         node beta \"Beta\" {\n  status \"deferred\"\n  reason \"later\"\n}\n",
        json!([{
            "code": "I905", "severity": "info", "check": "conditional_field_required",
            "message_template": "I905 '{id}' is deferred with no reason",
            "target_kind": "node", "field": "reason",
            "constraint": { "kind": "matches", "pattern": "status", "values": ["deferred"] }
        }]),
    );

    assert_eq!(
        messages(&diagnostics, "I905"),
        ["I905 'alpha' is deferred with no reason"]
    );
    assert!(
        naming(&diagnostics, "W112", "I905").is_empty(),
        "{diagnostics:?}"
    );
    assert_eq!(
        messages(&diagnostics, "W147"),
        [
            "extension '@pin/rules': rule 'I905': constraint kind 'matches' is not read by check 'conditional_field_required' (it reads when_field_equals) — read as when_field_equals"
        ]
    );
}
