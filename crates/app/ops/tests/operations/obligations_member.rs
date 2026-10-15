//! A field named `verify` and an entity's `verify` statements are two things (ADR 0094): the export writes
//! both, and the field map holds only the field.

use super::compiled_support::{TestProject, exported, published};
use serde_json::json;
use specforge_ops::read::export::Format;

const BOTH: &str = "type Payload \"Payload\" {\n  verify string @optional\n  name   string\n  verify unit \"Payload schema is valid\"\n}\n";
const FIELD_ONLY: &str =
    "type Payload \"Payload\" {\n  verify string @optional\n  name   string\n}\n";

fn project(source: &str) -> specforge_project::CompiledProject {
    TestProject::new()
        .enabling(&["@specforge/software", "@specforge/testing"])
        .files(&[("main.spec", source)])
        .compile()
}

#[test]
#[ignore = "flipped by T7"]
fn a_field_named_verify_is_exported_beside_the_statements() {
    let p = project(BOTH);
    let document = exported(&p);
    let node = document["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "Payload")
        .unwrap();
    assert_eq!(node["fields"]["verify"], "string", "{node}");
    assert_eq!(node["fields"]["name"], "string", "{node}");
    assert_eq!(
        node["verify"],
        json!([{"kind": "unit", "description": "Payload schema is valid"}]),
        "{node}"
    );
}

#[test]
#[ignore = "flipped by T7"]
fn an_export_with_a_field_named_verify_meets_its_published_schema() {
    let p = project(FIELD_ONLY);
    let violations =
        specforge_common::shape::violations(&published(&p, Format::Graph), &exported(&p));
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
#[ignore = "flipped by T8"]
fn a_field_named_verify_is_one_field_of_the_node() {
    let p = project(BOTH);
    let node = p.graph().node("Payload").unwrap();
    let keys: Vec<&str> = node
        .fields
        .entries()
        .iter()
        .map(|e| e.key.as_str())
        .collect();
    assert_eq!(keys, ["verify", "name"]);
}
