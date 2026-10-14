//! How every reader after the graph build sees an entity (ADR 0019): the
//! field text a declarative rule matches, a custom validator receives and a
//! compiler pass reads; who owes obligations under an untargeted obligation
//! rule; and where `file_exists` resolves a path. The probe extension
//! `@pin/snapshot` declares, in process, the rules that echo what each
//! reader saw (plan 01 §3, Appendix A).

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_project::CompiledProject;
use specforge_project::testing::TestProject;
use specforge_wasm::testing::InProcessRuntime;

const EXT: &str = "@pin/snapshot";

/// The fields the `P2xx` rules read, in code order (`P200` is `values`).
const FIELDS: &[&str] = &[
    "values",
    "mix",
    "ensures",
    "metric",
    "empty_block",
    "tags",
    "shape",
];

/// `item` (testable, accepts verify, `abstract` exempts), `note` (not
/// testable, accepts verify) and `memo` (neither); `P100` a custom rule answering `Pass` (its
/// inputs are read from the runtime's calls); `P200`–`P206` rules that
/// never match, on the fields of [`FIELDS`]; `P300` an obligation rule
/// with no target kind; `P400` `file_exists` on `doc`; a check pass `echo`
/// that reports nothing.
fn extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(EXT, "0.1.0"));
    c.kind("item", |k| {
        k.description("probe")
            .testable(true)
            .supports_verify(true)
            .open_fields(true);
        k.field("abstract", |f| {
            f.field_type(FieldType::Bool).exempts_obligations();
        });
    });
    c.kind("note", |k| {
        k.description("probe, not testable")
            .testable(false)
            .supports_verify(true)
            .open_fields(true);
    });
    c.kind("memo", |k| {
        k.description("probe, accepts no verify").open_fields(true);
    });
    c.rule("P100", |r| {
        r.check(CheckKind::Custom)
            .severity(ValidationSeverity::Info)
            .target_kind("item")
            .wasm_function("validate__echo")
            .message_template("custom validator sees {id}: {value}")
            .validate(|_| ValidatorVerdict::Pass);
    });
    for (i, field) in FIELDS.iter().enumerate() {
        c.rule(&format!("P2{i:02}"), |r| {
            r.check(CheckKind::FieldValueConstraint)
                .severity(ValidationSeverity::Info)
                .target_kind("item")
                .field(field)
                .message_template("declarative rule sees {id}.{field} = '{value}'")
                .constraint(|k| {
                    k.kind(ConstraintKind::Matches).pattern("^__never__$");
                });
        });
    }
    c.rule("P300", |r| {
        r.check(CheckKind::NoVerifyStatements)
            .severity(ValidationSeverity::Warning)
            .field("verify")
            .message_template("{kind} '{id}' declares no verify obligations");
    });
    c.rule("P400", |r| {
        r.check(CheckKind::FileExists)
            .severity(ValidationSeverity::Warning)
            .target_kind("item")
            .field("doc")
            .message_template("{id}: file '{value}' does not exist");
    });
    c.pass("echo", |p| {
        p.phase("check")
            .run(|_: &PassInput| Vec::<PassDiagnostic>::new());
    });
    c
}

const SPEC: &str = r#"item alpha "Alpha" {
  values [low, high]
  mix [1, true]
  ensures {
    done "it is done"
  }
  empty_block {
  }
  tags []
  doc "doc.md"
  metric expr { latency < 10ms, load > 5 }
  shape string | string[]
  verify unit "alpha works"
}

item beta "Beta" {
  values []
}

note gamma "Gamma" {
}

item delta "Delta" {
  abstract true
}

memo epsilon "Epsilon" {
}
"#;

/// The §3 project: `spec_root: "spec"`, `spec/main.spec` and `spec/doc.md`.
fn project() -> TestProject {
    TestProject::new()
        .config(|c| {
            c["name"] = "repro".into();
            c["spec_root"] = "spec".into();
        })
        .enabling(&[EXT])
        .installing_enabled()
        .files(&[("spec/main.spec", SPEC), ("spec/doc.md", "# doc\n")])
}

fn runtime() -> std::sync::Arc<InProcessRuntime> {
    std::sync::Arc::new(InProcessRuntime::new().with(extension))
}

/// A compile of the project through `runtime`, and its diagnostics.
fn compile(
    dir: &TestProject,
    runtime: &std::sync::Arc<InProcessRuntime>,
) -> (CompiledProject, Vec<Diagnostic>) {
    let compiled = dir.compile_in(runtime.clone());
    let diagnostics = compiled.diagnostics();
    (compiled, diagnostics)
}

/// `code message` of every diagnostic whose code starts with `prefix`.
fn reported(diagnostics: &[Diagnostic], prefix: &str) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code.starts_with(prefix))
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

/// The fields the custom validator received for `id`, by key.
fn validator_fields(runtime: &InProcessRuntime, id: &str) -> serde_json::Map<String, Value> {
    let call = runtime
        .calls()
        .into_iter()
        .find(|c| c.export == "validate__echo" && c.input["entity"]["id"] == id)
        .unwrap_or_else(|| panic!("validate__echo was never called for {id}"));
    call.input["entity"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["key"].as_str().unwrap().to_string(), f["value"].clone()))
        .collect()
}

/// The pass input's entity `id`.
fn pass_entity(runtime: &InProcessRuntime, id: &str) -> Value {
    let input = runtime
        .calls()
        .into_iter()
        .rev()
        .find(|c| c.export == "__pass_echo")
        .expect("the echo pass ran")
        .input;
    input["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no pass entity {id}"))
        .clone()
}

/// `alpha`'s fields as §3.1 expects every reader to see them.
const ALPHA: &[(&str, &str)] = &[
    ("values", "low | high"),
    ("mix", "1, true"),
    ("ensures", "done"),
    ("empty_block", ""),
    ("tags", ""),
    ("doc", "doc.md"),
    ("metric", "latency < 10ms, load > 5"),
    ("shape", "string | string[]"),
    ("verify", "alpha works"),
];

#[specforge_test_macros::test(
    behavior = "snapshot_entities_once",
    verify = "every field an entity writes has one text, the same for declarative rules, custom validators and compiler passes"
)]
fn every_field_an_entity_writes_has_one_text() {
    let dir = project();
    let runtime = runtime();
    let (_, diagnostics) = compile(&dir, &runtime);

    let expected: serde_json::Map<String, Value> = ALPHA
        .iter()
        .map(|(key, text)| (key.to_string(), Value::from(*text)))
        .collect();
    // The custom validator and the pass see the same texts, key by key.
    assert_eq!(validator_fields(&runtime, "alpha"), expected);
    assert_eq!(
        pass_entity(&runtime, "alpha")["fields"],
        Value::Object(expected)
    );
    let beta = serde_json::Map::from_iter([("values".to_string(), Value::from(""))]);
    assert_eq!(validator_fields(&runtime, "beta"), beta);
    assert_eq!(pass_entity(&runtime, "beta")["fields"], Value::Object(beta));

    // And so does every declarative rule: one P2xx per written field.
    let mut rules: Vec<String> = FIELDS
        .iter()
        .enumerate()
        .filter_map(|(i, field)| {
            let text = ALPHA.iter().find(|(key, _)| key == field)?.1;
            Some(format!(
                "P2{i:02} declarative rule sees alpha.{field} = '{text}'"
            ))
        })
        .collect();
    rules.push("P200 declarative rule sees beta.values = ''".to_string());
    let mut seen = reported(&diagnostics, "P2");
    seen.sort();
    rules.sort();
    assert_eq!(seen, rules);
}

/// `item`, with rules reading the written-but-empty `values` and
/// `requires`: `non_empty` (E100, E101) and `missing_required_field`
/// (E102, E103), and a custom validator (E104).
fn empty_values_extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@pin/empty", "0.1.0"));
    c.kind("item", |k| {
        k.description("probe").open_fields(true);
    });
    for (code, field) in [("E100", "values"), ("E101", "requires")] {
        c.rule(code, |r| {
            r.check(CheckKind::FieldValueConstraint)
                .target_kind("item")
                .field(field)
                .message_template("{id}.{field} is empty")
                .constraint(|k| {
                    k.kind(ConstraintKind::NonEmpty);
                });
        });
    }
    for (code, field) in [("E102", "values"), ("E103", "requires")] {
        c.rule(code, |r| {
            r.check(CheckKind::MissingRequiredField)
                .target_kind("item")
                .field(field)
                .message_template("{id} does not write {field}");
        });
    }
    c.rule("E104", |r| {
        r.check(CheckKind::Custom)
            .target_kind("item")
            .wasm_function("validate__empty")
            .message_template("unused")
            .validate(|_| ValidatorVerdict::Pass);
    });
    c
}

#[specforge_test_macros::test(
    behavior = "snapshot_entities_once",
    verify = "an empty list or block is written, with empty text, never left out or null"
)]
fn an_empty_list_or_block_is_written() {
    let dir = TestProject::new()
        .config(|c| c["name"] = "empty".into())
        .enabling(&["@pin/empty"])
        .installing_enabled()
        .file(
            "a.spec",
            "item zeta \"Zeta\" {\n  values []\n  requires {\n  }\n}\n",
        );
    let runtime = std::sync::Arc::new(InProcessRuntime::new().with(empty_values_extension));
    let (_, diagnostics) = compile(&dir, &runtime);

    // Written, so present (no E102/E103), and empty (E100/E101 fire).
    assert_eq!(
        reported(&diagnostics, "E10"),
        ["E100 zeta.values is empty", "E101 zeta.requires is empty"]
    );
    let call = runtime
        .calls()
        .into_iter()
        .find(|c| c.export == "validate__empty" && c.input["entity"]["id"] == "zeta")
        .expect("the validator saw zeta");
    assert_eq!(
        call.input["entity"]["fields"],
        json!([
            {"key": "values", "value": "", "annotations": []},
            {"key": "requires", "value": "", "annotations": []},
        ])
    );
}

/// The project's coverage, as the compile records it.
fn testable_total(compiled: &CompiledProject, root: &Path) -> usize {
    compiled
        .recorded()
        .at(Some(root))
        .unwrap()
        .coverage
        .summary
        .testable_total
}

#[specforge_test_macros::test(
    behavior = "snapshot_entities_once",
    verify = "a rule without a target kind applies to every kind, for the rule, the standing and the verify stub alike"
)]
fn an_untargeted_obligation_rule_obliges_every_kind() {
    let dir = project();
    let runtime = runtime();
    let (compiled, diagnostics) = compile(&dir, &runtime);

    assert_eq!(
        reported(&diagnostics, "P3"),
        [
            "P300 item 'beta' declares no verify obligations",
            "P300 note 'gamma' declares no verify obligations",
        ]
    );
    // The pass input agrees with the rule: alpha, beta and gamma owe
    // obligations (alpha declares one); delta's flag exempts it.
    for (id, exempt) in [
        ("alpha", false),
        ("beta", false),
        ("gamma", false),
        ("delta", true),
    ] {
        assert_eq!(pass_entity(&runtime, id)["owes_none"], exempt, "{id}");
    }
    // And so does coverage: alpha and beta are testable and owe (gamma's
    // kind is not testable).
    assert_eq!(testable_total(&compiled, dir.root()), 2);
}

#[specforge_test_macros::test(
    behavior = "snapshot_entities_once",
    verify = "a kind that accepts no verify statements owes no obligations, whatever rule applies to it"
)]
fn an_untargeted_obligation_rule_skips_kinds_that_accept_no_verify() {
    let dir = project();
    let runtime = runtime();
    let (compiled, diagnostics) = compile(&dir, &runtime);

    // `memo` accepts no `verify`: P300 does not ask epsilon for one.
    assert!(
        !reported(&diagnostics, "P3")
            .iter()
            .any(|d| d.contains("epsilon")),
        "{diagnostics:?}"
    );
    assert_eq!(pass_entity(&runtime, "epsilon")["owes_none"], true);
    assert_eq!(pass_entity(&runtime, "epsilon")["testable"], false);
    assert_eq!(testable_total(&compiled, dir.root()), 2);
}

#[specforge_test_macros::test(
    behavior = "execute_validation_pattern",
    verify = "file_exists resolves a relative path against the spec root, never the working directory"
)]
fn file_exists_resolves_against_the_spec_root() {
    let dir = project();
    // The test runs in the crate directory, where no `doc.md` is: only the
    // spec root holds one.
    assert!(!Path::new("doc.md").exists());
    let (_, diagnostics) = compile(&dir, &runtime());
    assert!(reported(&diagnostics, "P4").is_empty(), "{diagnostics:?}");

    fs::remove_file(dir.root().join("spec/doc.md")).unwrap();
    let (_, diagnostics) = compile(&dir, &runtime());
    assert_eq!(
        reported(&diagnostics, "P4"),
        ["P400 alpha: file 'doc.md' does not exist"]
    );
}

#[specforge_test_macros::test(
    behavior = "snapshot_entities_once",
    verify = "the checks, the check passes and the coverage of one compile read one snapshot"
)]
fn the_checks_passes_and_coverage_of_one_compile_read_one_snapshot() {
    let dir = project();
    let runtime = runtime();
    let (compiled, _) = compile(&dir, &runtime);
    let entities = compiled.entities();

    // The coverage memo holds the compile's snapshot, and the coverage is
    // computed from it: no second walk.
    let memo = compiled.recorded();
    assert!(std::ptr::eq(entities, &**memo.entities()));
    let recorded = memo.at(Some(dir.root())).unwrap();
    assert!(
        recorded
            .coverage
            .verdicts
            .keys()
            .eq(entities.records().iter().map(|r| &r.id)),
        "the coverage scores the compile's snapshot"
    );

    // The check pass received the snapshot's adapter, entity by entity.
    let pass_input = runtime
        .calls()
        .into_iter()
        .find(|c| c.export == "__pass_echo")
        .expect("the echo pass ran")
        .input;
    assert_eq!(
        pass_input["entities"],
        serde_json::to_value(entities.pass_entities()).unwrap()
    );
    assert_eq!(
        pass_input["edges"],
        serde_json::to_value(entities.pass_edges()).unwrap()
    );

    // And each custom validator call its context.
    let mut validated = 0;
    for call in runtime
        .calls()
        .into_iter()
        .filter(|c| c.export == "validate__echo" && c.input["entity"]["id"] != "__probe__")
    {
        let id = call.input["entity"]["id"].as_str().unwrap().to_string();
        let context = entities.validator_context(&id).expect("a snapshot entity");
        assert_eq!(call.input, serde_json::to_value(context).unwrap(), "{id}");
        validated += 1;
    }
    assert_eq!(validated, 3, "alpha, beta and delta are items");
}
