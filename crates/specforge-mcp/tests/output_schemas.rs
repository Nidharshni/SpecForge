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
        .filter_map(|tool| {
            tool.output_schema()
                .map(|schema| (tool.name(), schema.clone()))
        })
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

/// How many object schemas of `schema` (its `$defs` included) look like a
/// diagnostic.
fn diagnostic_bodies(schema: &Value) -> usize {
    let mut found = Vec::new();
    objects(schema, &mut found);
    found
        .iter()
        .filter(|object| {
            let properties = &object["properties"];
            ["code", "title", "severity", "message", "span"]
                .iter()
                .all(|key| properties.get(key).is_some())
        })
        .count()
}

#[specforge_test(
    behavior = "follow_negotiated_mcp_revision",
    verify = "a type a reply holds more than once, or a domain type another spec file declares, is defined once under the outputSchema's $defs and referred to by $ref"
)]
fn each_named_shape_is_defined_once_per_document() {
    let mut holding = BTreeMap::new();
    for (name, schema) in documents() {
        let bodies = diagnostic_bodies(&schema);
        if bodies == 0 {
            continue;
        }
        // One body, under $defs, and the property schemas refer to it.
        assert_eq!(bodies, 1, "{name} holds a diagnostic body more than once");
        let defs = &schema["$defs"];
        assert!(defs["Diagnostic"].is_object(), "{name}: {defs}");
        assert!(defs["SourceSpan"].is_object(), "{name}: {defs}");
        let outside = {
            let mut without = schema.clone();
            without.as_object_mut().expect("an object").remove("$defs");
            diagnostic_bodies(&without)
        };
        assert_eq!(
            outside, 0,
            "{name} holds an inline diagnostic outside $defs"
        );
        holding.insert(name.trim_start_matches("specforge."), ());
    }
    let expected: Vec<&str> = vec![
        "analyze",
        "collect",
        "format",
        "inspect",
        "migrate",
        "providers",
        "rename",
    ];
    assert_eq!(holding.keys().copied().collect::<Vec<_>>(), expected);
    let migrate = documents()
        .into_iter()
        .find(|(name, _)| *name == "specforge.migrate")
        .expect("migrate lists a schema")
        .1;
    let lists: Vec<&Value> = [
        "post_migration_errors",
        "schema_warnings",
        "structural_differences",
    ]
    .iter()
    .map(|key| &migrate["properties"][key])
    .collect();
    assert_eq!(lists.len(), 3, "{migrate}");
    for list in lists {
        assert_eq!(
            list["items"],
            serde_json::json!({"$ref": "#/$defs/Diagnostic"})
        );
    }
}

/// Every `{"$ref": …}` of `schema`, with its path.
fn references<'v>(schema: &'v Value, path: &str, out: &mut Vec<(String, &'v Value)>) {
    match schema {
        Value::Object(map) => {
            if map.contains_key("$ref") {
                out.push((path.to_string(), schema));
            }
            for (key, child) in map {
                references(child, &format!("{path}.{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                references(item, &format!("{path}[{index}]"), out);
            }
        }
        _ => {}
    }
}

#[specforge_test(
    behavior = "state_mcp_messages_as_spec_types",
    verify = "a sent document's schema closes every object it holds but its listed open values, and repeats no object schema"
)]
fn no_object_schema_repeats_within_a_document() {
    for document in crate::support::documents::all() {
        let (name, schema) = (document.owner, document.schema);
        let mut found = Vec::new();
        objects(schema, &mut found);
        let mut seen = std::collections::BTreeSet::new();
        for object in found {
            assert!(
                seen.insert(object.to_string()),
                "{name} holds an object schema twice: {object}"
            );
        }
    }
}

#[test]
fn a_reference_stands_alone() {
    for (name, schema) in documents() {
        let mut found = Vec::new();
        references(&schema, "$", &mut found);
        for (path, reference) in found {
            assert_eq!(
                reference.as_object().map(serde_json::Map::len),
                Some(1),
                "{name} {path}: a reference has siblings: {reference}"
            );
        }
    }
}

#[specforge_test(
    behavior = "follow_negotiated_mcp_revision",
    verify = "every $ref in a core tool's outputSchema is a same-document pointer to a definition it holds"
)]
fn every_listed_reference_resolves() {
    let mut total = 0;
    for (name, schema) in documents() {
        let mut found = Vec::new();
        references(&schema, "$", &mut found);
        for (path, reference) in found {
            total += 1;
            let target = reference["$ref"].as_str().expect("a string reference");
            let pointer = target
                .strip_prefix("#/$defs/")
                .unwrap_or_else(|| panic!("{name} {path}: {target} is no $defs pointer"));
            assert!(
                schema["$defs"][pointer].is_object(),
                "{name} {path}: {target} names no definition"
            );
        }
    }
    assert!(total > 0, "no core outputSchema refers to a named shape");
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

#[test]
fn a_tools_output_schema_is_built_once() {
    let mut typed = 0;
    for tool in specforge_mcp::tools::CORE_TOOLS {
        let Some(first) = tool.output_schema() else {
            continue;
        };
        typed += 1;
        let again = tool.output_schema().expect("a typed tool lists a schema");
        assert!(
            std::ptr::eq(first, again),
            "{}'s outputSchema was built twice",
            tool.name()
        );
        assert_eq!(
            tool.descriptor().output_schema.as_ref(),
            Some(first),
            "{}'s descriptor lists another schema",
            tool.name()
        );
    }
    assert!(typed > 0);
}
