//! What every core tool replies today, defects included (plan 11).
//!
//! These pins are unlinked: they hold the replies as they are, `null`s and
//! undeclared keys among them. The ticket that changes what a pin holds
//! re-blesses its snapshot in the same commit, so the diff is the
//! user-visible change.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use specforge_test::prelude::*;

use crate::support::replies::{Observed, read_all, write_all};

/// An object as `{k: shape, …}` with sorted keys, an array as
/// `[shape | shape …]` (the distinct element shapes, sorted), a scalar as
/// its JSON type.
fn shape(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(_) => "boolean".into(),
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer".into(),
        Value::Number(_) => "number".into(),
        Value::String(_) => "string".into(),
        Value::Array(items) => {
            let mut shapes: Vec<String> = items.iter().map(shape).collect();
            shapes.sort();
            shapes.dedup();
            format!("[{}]", shapes.join(" | "))
        }
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys
                .into_iter()
                .map(|key| format!("{key}: {}", shape(&object[key])))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}

fn observed() -> Vec<Observed> {
    let mut all = read_all();
    all.extend(write_all());
    all
}

#[test]
fn reply_shapes_today() {
    let lines: Vec<String> = observed()
        .iter()
        .map(|call| {
            format!(
                "{} {} => {}",
                call.tool,
                call.arguments,
                shape(&call.reply())
            )
        })
        .collect();
    insta::assert_snapshot!(lines.join("\n"));
}

/// The paths of `schema` that are open: an object without `properties` or
/// without `additionalProperties: false`, an array without `items`, a value
/// of any shape (`{}`). A union is open where its branches are.
fn open_paths(schema: &Value, path: &str, found: &mut BTreeSet<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if object.is_empty() {
        found.insert(path.to_string());
        return;
    }
    let mut composed = false;
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = schema.get(key).and_then(Value::as_array) {
            composed = true;
            for branch in branches {
                open_paths(branch, path, found);
            }
        }
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if types.contains(&"object") || schema.get("properties").is_some() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if !composed
            && (properties.is_none() || schema.get("additionalProperties") != Some(&json!(false)))
        {
            found.insert(path.to_string());
        }
        for (key, property) in properties.into_iter().flatten() {
            open_paths(property, &format!("{path}.{key}"), found);
        }
    }
    if types.contains(&"array") {
        match schema.get("items") {
            Some(items) => open_paths(items, &format!("{path}[]"), found),
            None => {
                found.insert(format!("{path}[] (no items)"));
            }
        }
    }
}

#[specforge_test(
    behavior = "follow_negotiated_mcp_revision",
    verify = "a core tool's output schema is derived from its typed reply: every object it closes lists its keys, every array its items"
)]
fn every_core_output_schema_is_closed() {
    let mut found = BTreeSet::new();
    for spec in specforge_mcp::tools::CORE_TOOLS {
        let Some(schema) = spec.output_schema() else {
            continue;
        };
        let mut paths = BTreeSet::new();
        open_paths(&schema, "$", &mut paths);
        for path in paths {
            found.insert((spec.name().to_string(), path));
        }
    }
    let listed: BTreeSet<(String, String)> = specforge_mcp::tools::CORE_TOOLS
        .iter()
        .flat_map(|tool| {
            tool.open_values()
                .iter()
                .map(move |path| (tool.name().to_string(), (*path).to_string()))
        })
        .collect();
    let unlisted: Vec<_> = found.difference(&listed).collect();
    let stale: Vec<_> = listed.difference(&found).collect();
    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "open and not listed: {unlisted:#?}\nlisted and no longer open: {stale:#?}"
    );
}

/// The reply of `tool` on the fixture, whole: the call's `result`.
fn result_of(tool: &str, arguments: Value) -> Value {
    let mut served = crate::support::replies::reading();
    crate::support::call_tool(&mut served, tool, arguments)["result"].clone()
}

/// `result` is an object holding `rows` under `key`, sent as the
/// structured result as well as the text.
fn holds_rows(result: &Value, key: &str) {
    assert_eq!(result["isError"], false, "{result}");
    let structured = &result["structuredContent"];
    assert!(structured[key].is_array(), "{key}: {result}");
    let text: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().expect("a text")).unwrap();
    assert_eq!(&text, structured, "the text is the structured result");
}

#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "the result is an object holding the hits as results, with structuredContent"
)]
fn search_answers_an_object() {
    holds_rows(
        &result_of("specforge.search", json!({"query": "a"})),
        "results",
    );
}

#[specforge_test(
    behavior = "provide_mcp_entities_by_kind",
    verify = "the result is an object holding the entities"
)]
fn list_answers_an_object() {
    holds_rows(&result_of("specforge.list", json!({})), "entities");
}

#[specforge_test(
    behavior = "provide_mcp_coverage_tool",
    verify = "the result is an object holding the rows as entities"
)]
fn coverage_answers_an_object() {
    holds_rows(&result_of("specforge.coverage", json!({})), "entities");
}

#[specforge_test(
    behavior = "provide_mcp_outline_tool",
    verify = "the result is an object holding the outline as entries"
)]
fn outline_answers_an_object() {
    holds_rows(
        &result_of("specforge.outline", json!({"file": "test.spec"})),
        "entries",
    );
}

#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "the result is an object holding the fixes"
)]
fn suggest_fixes_answers_an_object() {
    holds_rows(&result_of("specforge.suggest_fixes", json!({})), "fixes");
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "an untitled entity's reply has no title"
)]
fn an_untitled_entity_has_no_title() {
    let reply = result_of("specforge.inspect", json!({"entity_id": "untitled_one"}));
    let structured = reply["structuredContent"].as_object().expect("an object");
    assert!(!structured.contains_key("title"), "{structured:?}");
    // A reply never carries null: an absent value is an absent key.
    for key in ["source_extension", "contract"] {
        assert_ne!(structured.get(key), Some(&Value::Null), "{key}");
    }
}

#[specforge_test(
    behavior = "provide_mcp_inspect_tool",
    verify = "the entity's diagnostics are in the shape validate reports them"
)]
fn inspect_diagnostics_are_validates() {
    let mut served = crate::support::replies::reading();
    let inspected = crate::support::tool(
        &mut served,
        "specforge.inspect",
        json!({"entity_id": "untitled_one"}),
    );
    let validated = crate::support::tool(&mut served, "specforge.validate", json!({}));
    let validated = validated.as_array().expect("validate answers an array");
    let diagnostics = inspected["diagnostics"].as_array().expect("an array");
    assert!(!diagnostics.is_empty(), "{inspected}");
    for diagnostic in diagnostics {
        let same = validated
            .iter()
            .find(|v| v["code"] == diagnostic["code"] && v["span"] == diagnostic["span"])
            .unwrap_or_else(|| panic!("validate reports no {diagnostic}"));
        assert_eq!(same, diagnostic);
        // The full keys: the catalogue's title, the span, the flat position.
        for key in ["title", "span", "file", "line", "column"] {
            assert!(diagnostic.get(key).is_some(), "{key}: {diagnostic}");
        }
    }
}
