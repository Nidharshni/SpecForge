//! What every core tool's outputSchema is (plan 05, ADR 0051).
//!
//! The pins are unlinked: they hold the schemas as they are. The ticket that
//! changes what a pin holds flips it in the same commit.

use std::collections::BTreeMap;

use serde_json::Value;
use specforge_test::prelude::*;

/// Every core tool with an outputSchema, by name.
fn documents() -> Vec<(&'static str, Value)> {
    specforge_mcp::tools::CORE_TOOLS
        .iter()
        .filter_map(|tool| tool.output_schema().map(|schema| (tool.name, schema)))
        .collect()
}

/// Every object schema with properties under `schema` (its `$defs` included).
fn objects<'v>(schema: &'v Value, out: &mut Vec<&'v Value>) {
    match schema {
        Value::Object(map) => {
            if map.get("properties").is_some_and(Value::is_object) {
                out.push(schema);
            }
            for child in map.values() {
                objects(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| objects(item, out)),
        _ => {}
    }
}

#[specforge_test(
    behavior = "follow_negotiated_mcp_revision",
    verify = "every core tool's outputSchema is an object schema at its root"
)]
fn every_output_schema_root_is_an_object() {
    for (name, schema) in documents() {
        assert_eq!(
            schema.get("type"),
            Some(&Value::from("object")),
            "{name}'s outputSchema root states no type object"
        );
    }
}

#[test]
fn inline_diagnostic_definitions_today() {
    let mut counts = BTreeMap::new();
    for (name, schema) in documents() {
        let mut found = Vec::new();
        objects(&schema, &mut found);
        let diagnostics = found
            .iter()
            .filter(|object| {
                let properties = &object["properties"];
                ["code", "title", "severity", "message", "span"]
                    .iter()
                    .all(|key| properties.get(key).is_some())
            })
            .count();
        if diagnostics > 0 {
            counts.insert(name.trim_start_matches("specforge."), diagnostics);
        }
    }
    let expected = BTreeMap::from([
        ("analyze", 1),
        ("collect", 1),
        ("format", 1),
        ("inspect", 1),
        ("migrate", 3),
        ("providers", 1),
        ("rename", 1),
    ]);
    assert_eq!(counts, expected);
}

/// The cost of the listing's output schemas in bytes, one line per tool: the
/// review diff states what a reply change spends (ADR 0051).
#[test]
fn core_output_schema_sizes() {
    let mut total = 0;
    let mut lines = Vec::new();
    for (name, schema) in documents() {
        let bytes = serde_json::to_string(&schema).expect("a schema").len();
        total += bytes;
        lines.push(format!("{name} {bytes}"));
    }
    lines.push(format!("total {total}"));
    insta::assert_snapshot!(lines.join("\n"));
}
