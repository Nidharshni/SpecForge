//! `spec/events/*.spec` declares every event the server records, its payload
//! exactly the variant's fields plus the sink's `timestamp` (ADR 0057).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use specforge_mcp::events::McpEvent;
use specforge_parser::FieldValue;
use specforge_test::prelude::*;

use crate::support::spec_files;

/// A payload field as the spec declares it: name, type text, optional.
type Declared = (String, String, bool);

/// Each event `spec/events/*.spec` declares: its file and its payload
/// fields.
fn declared() -> BTreeMap<String, (String, Vec<Declared>)> {
    let mut events: BTreeMap<String, (String, Vec<Declared>)> = BTreeMap::new();
    for (path, file) in spec_files("events") {
        for entity in file
            .entities
            .iter()
            .filter(|entity| entity.kind.raw.as_str() == "event")
        {
            let fields = match entity.fields.get("payload") {
                Some(FieldValue::Block(payload)) => payload
                    .entries()
                    .iter()
                    .map(|entry| {
                        let ty = match &entry.value {
                            FieldValue::Identifier(text) => text.clone(),
                            other => panic!(
                                "{}.{}: a payload type is an identifier, not {other:?}",
                                entity.id.raw, entry.key
                            ),
                        };
                        let optional = entry.annotations.iter().any(|a| a.name == "optional");
                        (entry.key.to_string(), ty, optional)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let name = entity.id.raw.to_string();
            if let Some((first, _)) =
                events.insert(name.clone(), (path.display().to_string(), fields))
            {
                panic!("{name} is declared in {first} and {}", path.display());
            }
        }
    }
    events
}

/// `spec/types` aliases of string literals (`McpToolCategory`): name to
/// members.
fn aliases() -> BTreeMap<String, BTreeSet<String>> {
    let mut aliases = BTreeMap::new();
    for (_, file) in spec_files("types") {
        for entity in file
            .entities
            .iter()
            .filter(|entity| entity.kind.raw.as_str() == "type")
        {
            if let Some(FieldValue::VariantList(members)) =
                entity.fields.get(specforge_parser::UNION_VARIANTS_FIELD)
            {
                aliases.insert(entity.id.raw.to_string(), members.iter().cloned().collect());
            }
        }
    }
    aliases
}

/// Whether `schema` is what the spec type `ty` says.
fn agrees(ty: &str, schema: &Value, aliases: &BTreeMap<String, BTreeSet<String>>) -> bool {
    match ty {
        "string" | "integer" | "boolean" => schema["type"] == ty,
        alias => aliases.get(alias).is_some_and(|members| {
            schema["enum"].as_array().is_some_and(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect::<BTreeSet<_>>()
                    == *members
            })
        }),
    }
}

#[specforge_test(
    behavior = "mcp_event_record",
    verify = "every event the server records is one an events spec declares, its payload exactly the declared fields"
)]
fn every_recorded_event_is_declared_with_exactly_its_payload() {
    let declared = declared();
    let aliases = aliases();
    let document = specforge_common::shape::document::<McpEvent>();
    let mut violations = Vec::new();
    let mut recorded = BTreeSet::new();
    for branch in document["oneOf"]
        .as_array()
        .expect("McpEvent is a tagged union")
    {
        let properties = branch["properties"].as_object().unwrap();
        let name = properties["event"]["enum"][0].as_str().unwrap().to_string();
        recorded.insert(name.clone());
        let Some((_, fields)) = declared.get(&name) else {
            violations.push(format!("{name}: no spec/events file declares it"));
            continue;
        };
        // The keeping sink stamps every event.
        if !fields
            .iter()
            .any(|(f, ty, optional)| f == "timestamp" && ty == "timestamp" && !optional)
        {
            violations.push(format!(
                "{name}: its payload must declare `timestamp timestamp`"
            ));
        }
        let required: BTreeSet<&str> = branch["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for (field, ty, optional) in fields.iter().filter(|(f, ..)| f != "timestamp") {
            let Some(schema) = properties.get(field) else {
                violations.push(format!("{name}.{field}: declared, never recorded"));
                continue;
            };
            if required.contains(field.as_str()) == *optional {
                violations.push(format!(
                    "{name}.{field}: the spec says optional = {optional}, the server sends it {}",
                    if *optional {
                        "always"
                    } else {
                        "only sometimes"
                    }
                ));
            }
            if !agrees(ty, schema, &aliases) {
                violations.push(format!("{name}.{field}: declared {ty}, recorded {schema}"));
            }
        }
        for key in properties.keys().filter(|k| *k != "event") {
            if !fields.iter().any(|(f, ..)| f == key) {
                violations.push(format!("{name}.{key}: recorded, not declared"));
            }
        }
    }
    // MCP's own event files: every event they declare is one the server
    // records.
    for (name, (file, _)) in &declared {
        let ours = file.ends_with("events/mcp.spec")
            || file.ends_with("events/surface-contributions.spec");
        if ours && !recorded.contains(name) {
            violations.push(format!("{name} ({file}): declared, never recorded"));
        }
    }
    assert!(violations.is_empty(), "{violations:#?}");
    assert_eq!(recorded.len(), 19, "{recorded:?}");
}
