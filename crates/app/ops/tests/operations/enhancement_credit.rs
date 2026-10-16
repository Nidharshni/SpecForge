//! An enhancement's fields are its declaring extension's (ADR 0096): trace does not expect them, the model names
//! their extension. Over the real builtins: software enhances product's `module`.

use serde_json::Value;
use specforge_emitter::model::{FieldLevel, ModelFormat, ModelOptions};
use specforge_ops::read::model::model;
use specforge_ops::view::ProjectView;
use specforge_project::CompiledProject;
use specforge_test_macros::test as specforge_test;

use super::compiled_support::{TestProject, trace_document};

fn compiled() -> CompiledProject {
    TestProject::new()
        .enabling(&["@specforge/product", "@specforge/software"])
        .config(|c| c["spec_root"] = "spec".into())
        .files(&[(
            "spec/main.spec",
            "module core_mod \"Core\" {\n}\nport store \"S\" {\n  direction outbound\n}\n",
        )])
        .compile()
}

#[specforge_test(
    behavior = "compute_traceability_chain",
    verify = "a reference field another extension adds to a kind is not an expected link"
)]
fn trace_does_not_expect_the_fields_another_extension_adds() {
    let compiled = compiled();
    let document = trace_document(&compiled, "core_mod");
    let missing: Vec<&str> = document["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["edge_label"].as_str().unwrap())
        .collect();
    assert_eq!(missing, ["features"], "{document}");
}

#[specforge_test(
    behavior = "build_model_intermediate",
    verify = "a field an entity enhancement adds names the extension that declared it"
)]
fn the_model_names_the_extension_that_added_a_field() {
    let compiled = compiled();
    let options = ModelOptions {
        format: ModelFormat::Json,
        fields: FieldLevel::All,
        ..ModelOptions::default()
    };
    let outcome = model(&ProjectView::of(&compiled), &options).expect("no refusal");
    let json: Value =
        serde_json::from_str(&outcome.into_value_and_notices().0).expect("the json model parses");
    let module = json["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "module")
        .expect("the module kind");
    for name in ["ports", "ports_defined"] {
        let field = module["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("module.{name}: {module}"));
        assert_eq!(field["contributed_by"], "@specforge/software", "{field}");
    }
    assert_eq!(
        module["enhanced_by"],
        serde_json::json!(["@specforge/software"]),
        "{module}"
    );
}
