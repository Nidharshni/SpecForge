//! Characterization pins for the prompts' read views (plan 04): what the
//! explore, review and infer prompts and `specforge.stats` answer today,
//! bugs included. The ticket that fixes each bug flips its pin in the same
//! commit. They are unlinked on purpose: they encode today's answers.

use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::*;
use specforge_mcp::McpServer;

/// `linked -features-> hub`, `lonely` with no reference, `dangling` whose
/// only reference does not resolve, `selfish` referencing itself, `alone`
/// with nothing.
fn connectivity_project() -> Served {
    TestProject::new()
        .file(
            "u.spec",
            "feature hub \"Hub\" {\n    problem \"Linked implements it\"\n}\n\n\
             behavior linked \"Linked\" {\n    features [hub]\n    verify unit \"linked works\"\n}\n\n\
             behavior lonely \"Lonely\" {\n    verify unit \"lonely works\"\n}\n\n\
             behavior dangling \"Dangling\" {\n    needs [nowhere]\n    verify unit \"dangling works\"\n}\n\n\
             feature selfish \"Selfish\" {\n    depends_on [selfish]\n}\n\n\
             feature alone \"Alone\" {\n}\n",
        )
        .serve(&[TestExtension::software()
            .obligating("behavior")
            .reference("behavior", "needs", "behavior")
            .reference("feature", "depends_on", "feature")])
}

fn explore(server: &mut McpServer, arguments: Value) -> Value {
    prompt_payload(&get_prompt(
        server,
        "specforge://prompts/explore",
        arguments,
    ))
}

fn infer(server: &mut McpServer, arguments: Value) -> Value {
    prompt_payload(&get_prompt(server, "specforge://prompts/infer", arguments))
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not an array: {value}"))
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn connectivity_is_one_rule() {
    let mut served = connectivity_project();
    let stats = tool(&mut served, "specforge.stats", json!({}));
    assert_eq!(stats["unconnected_count"], 4, "{stats}");

    let explored = explore(&mut served, json!({}));
    assert_eq!(
        strings(&explored["unconnected"]),
        ["alone", "dangling", "lonely", "selfish"]
    );
    assert_eq!(strings(&explored["high_connectivity"]), ["hub", "linked"]);
    assert_eq!(strings(&explored["starting_points"]), ["linked", "hub"]);

    let review = prompt_payload(&get_prompt(
        &mut served,
        "specforge://prompts/review",
        json!({}),
    ));
    let mut unconnected: Vec<&str> = review["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["message"].as_str().unwrap().contains("is unconnected"))
        .map(|f| f["entity_id"].as_str().unwrap())
        .collect();
    unconnected.sort_unstable();
    assert_eq!(unconnected, ["dangling", "lonely"]);
}

#[test]
fn explore_applies_its_selection_to_every_list() {
    let mut served = connectivity_project();
    let explored = explore(&mut served, json!({"kind": "feature"}));
    assert_eq!(strings(&explored["starting_points"]), ["hub"]);
    assert_eq!(strings(&explored["unconnected"]), ["alone", "selfish"]);

    let unknown = explore(&mut served, json!({"kind": "featur"}));
    assert_eq!(unknown["matching_entities"], json!([]));
    assert_eq!(unknown["notices"][0]["code"], "I020", "{unknown}");
}

/// `@specforge/test` declaring `widget`: a required `owner` reference to a
/// widget, a required `steps` string list and an optional `active` bool.
fn widget_extension() -> TestExtension {
    TestExtension::named("@specforge/test").declaring(|c| {
        c.kind("widget", |k| {
            k.field("owner", |f| {
                f.field_type(FieldType::Reference)
                    .target_kind("widget")
                    .required();
            });
            k.field("steps", |f| {
                f.field_type(FieldType::StringList).required();
            });
            k.field("active", |f| {
                f.field_type(FieldType::Bool);
            });
        });
    })
}

#[test]
fn infer_example_writes_each_field_by_its_type() {
    let mut served = TestProject::new().serve(&[widget_extension()]);
    let scoped = infer(&mut served, json!({"scope": "kind:widget"}));
    assert_eq!(
        scoped["example"],
        "widget example_widget \"Example Title\" {\n  owner ref_id\n  steps [\"item1\", \"item2\"]\n  active true\n}"
    );
}

/// `@specforge/test` declaring a `behavior` kind, a `w` entity under a
/// `model/` spec root.
fn model_root_project() -> Served {
    TestProject::new()
        .config(|c| c["spec_root"] = json!("model"))
        .file("model/w.spec", "behavior w {\n}\n")
        .serve(&[TestExtension::named("@specforge/test").declaring(|c| {
            c.kind("behavior", |k| {
                k.description("A behavior");
            });
        })])
}

#[test]
fn infer_names_the_projects_spec_directory() {
    let mut served = model_root_project();
    let overview = infer(&mut served, json!({}));
    assert!(
        overview["output_format"]
            .as_str()
            .unwrap()
            .contains("in the model/ directory"),
        "{overview}"
    );
    let plan = infer(&mut served, json!({"scope": "plan"}));
    assert_eq!(plan["scope"], "plan");
    assert_eq!(plan["target_spec_directory"], "model/");
}

#[test]
fn infer_names_tools_as_the_tool_table_does() {
    let mut served = model_root_project();
    let overview = infer(&mut served, json!({}));
    let kind = infer(&mut served, json!({"scope": "kind:behavior"}));
    let file = infer(&mut served, json!({"scope": "file:w.rs"}));
    for payload in [&overview, &kind, &file] {
        assert!(!payload.to_string().contains("specforge_"), "{payload}");
        assert!(
            payload["validation"]
                .as_str()
                .unwrap()
                .contains("specforge.validate"),
            "{payload}"
        );
    }
    let workflow = infer(&mut served, json!({"scope": "workflow"}));
    assert!(strings(&workflow["tools"]).contains(&"specforge.validate"));
}

#[test]
fn infer_plan_lists_unwritten_kinds_first_in_reference_order() {
    let extension = TestExtension::named("@specforge/test").declaring(|c| {
        c.kind("behavior", |k| {
            k.field("events", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("event");
            });
        });
        c.kind("event", |k| {
            k.field("payload", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("type");
            });
        });
        c.kind("type", |_| {});
    });
    let mut served = TestProject::new()
        .file("b.spec", "behavior b {\n}\n")
        .serve(&[extension]);
    let plan = infer(&mut served, json!({"scope": "plan"}));
    let kinds: Vec<&str> = plan["kind_priorities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["type", "event", "behavior"]);
}
