//! The standalone JSON Schema of an export (`specforge schema --publish`).
//!
//! It is the export document's own schema ([`crate::document_schema`],
//! derived from the types that write the export, ADR 0051) narrowed by the
//! project's Graph Protocol schema. A field's value schema is its field
//! form's ([`specforge_graph::FieldForm::json_schema`]), so what a published
//! field takes is what the writer writes for a value its check admits
//! (ADR-NEW-01).

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use specforge_common::shape::{Definitions, Shape};
use specforge_common::structural::NODE_KINDS;
use specforge_graph::field_form::FieldJson;

use crate::emit::{EmitFormat, document_schema};
use crate::schema::{GraphProtocolSchema, SchemaEntityKind, schema_url};

/// The standalone JSON Schema (draft 2020-12) of a `format` export of a
/// project whose Graph Protocol schema is `schema`; `None` for DOT. It is the
/// export document's own schema ([`crate::document_schema`], derived from the
/// types that write the export) narrowed by the project:
///
/// - root: `$schema`, `$id` (`…/graph-protocol-v{version}.json` for graph,
///   `…-v{version}-{context|brief}.json` otherwise), `title`, `description`;
///   the graph format requires `format_version`;
/// - graph nodes: one branch per kind an export can hold (every declared
///   kind, then the structural `ref` and `spec`), `$defs/node_<kind>`,
///   chosen by `oneOf`; a declared kind's `fields` is `$defs/fields_<kind>`:
///   each declared field's field form schema,
///   its required fields required, any other key a `FieldJson`;
/// - context and brief nodes: `kind` is one of those kinds;
/// - the embedded schema's edge type labels are the registered ones, and an
///   edge's `label` lists the fields that declare an edge type as `examples`.
///
/// With no declared kind (a structural-only project) the nodes stay as
/// derived. Every keyword is one [`specforge_common::shape::violations`]
/// reads or an annotation: [`specforge_common::shape::unchecked_keywords`] is
/// empty.
pub fn publish(schema: &GraphProtocolSchema, format: EmitFormat) -> Option<Value> {
    let mut defs = Definitions::default();
    let body = document_schema(format, &mut defs)?;
    let declared = !schema.entity_kinds.is_empty();

    // The per-kind field objects are built against the same definitions as
    // the document, so a block field's `FieldJson` is defined once.
    let fields: Vec<(&SchemaEntityKind, Value)> = match (format, declared) {
        (EmitFormat::Json, true) => schema
            .entity_kinds
            .iter()
            .map(|kind| (kind, fields_schema(kind, &mut defs)))
            .collect(),
        _ => Vec::new(),
    };
    let mut document = defs.finish(body);

    // The kinds an export can hold: declared, then the structural ones.
    let kinds: Vec<&str> = schema
        .entity_kinds
        .iter()
        .map(|kind| kind.name.as_str())
        .chain(NODE_KINDS)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let kinds = ordered(schema, kinds);

    if declared {
        match format {
            EmitFormat::Json => {
                let base = at(&mut document, "/properties/nodes/items").clone();
                let mut branches = Vec::new();
                let mut node_defs = Vec::new();
                for kind in &kinds {
                    let mut node = base.clone();
                    node["properties"]["kind"] = json!({"type": "string", "enum": [kind]});
                    if fields.iter().any(|(declared, _)| declared.name == *kind) {
                        node["properties"]["fields"] =
                            json!({"$ref": format!("#/$defs/fields_{kind}")});
                    }
                    node_defs.push((format!("node_{kind}"), node));
                    branches.push(json!({"$ref": format!("#/$defs/node_{kind}")}));
                }
                *at(&mut document, "/properties/nodes/items") = json!({"oneOf": branches});
                let defs = at(&mut document, "/$defs")
                    .as_object_mut()
                    .expect("$defs is an object");
                for (name, node) in node_defs {
                    defs.insert(name, node);
                }
                for (kind, fields) in fields {
                    defs.insert(format!("fields_{}", kind.name), fields);
                }
            }
            EmitFormat::Context | EmitFormat::Brief => {
                *at(&mut document, "/properties/nodes/items/properties/kind") =
                    json!({"type": "string", "enum": kinds});
            }
            EmitFormat::Dot => unreachable!("DOT has no document schema"),
        }
    }

    if !schema.edge_types.is_empty() {
        let labels: Vec<&str> = schema.edge_types.iter().map(|e| e.label.as_str()).collect();
        *at(
            &mut document,
            "/$defs/GraphProtocolSchema/properties/edge_types/items/properties/label",
        ) = json!({"type": "string", "enum": labels});
    }

    // A graph edge is labelled with the name of the field that declared the
    // reference (`invariants`, not `BehaviorEnforcesInvariant`). The label is
    // open: any list field whose entries name entities yields an edge,
    // including fields no extension declares, so it cannot be an enum. The
    // fields that declare an edge type are listed as examples.
    let edge_fields: BTreeSet<&str> = schema
        .entity_kinds
        .iter()
        .flat_map(|kind| &kind.fields)
        .filter(|field| field.edge.is_some())
        .map(|field| field.name.as_str())
        .collect();
    let label = at(&mut document, "/$defs/Edge/properties/label");
    label["description"] = json!(
        "Name of the field that declared the reference, for example `invariants`. Open: extensions add fields."
    );
    if !edge_fields.is_empty() {
        label["examples"] = json!(edge_fields);
    }

    let root = document.as_object_mut().expect("a document is an object");
    if format == EmitFormat::Json {
        let required = root
            .get_mut("required")
            .and_then(Value::as_array_mut)
            .expect("the export document requires its envelope");
        required.insert(0, json!("format_version"));
    }
    let version = &schema.schema_version;
    let (variant, what) = match format {
        EmitFormat::Json => (None, "graph"),
        EmitFormat::Context => (Some("context"), "context"),
        EmitFormat::Brief => (Some("brief"), "brief"),
        EmitFormat::Dot => unreachable!("DOT has no document schema"),
    };
    let mut annotated = Map::new();
    annotated.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    annotated.insert("$id".into(), json!(schema_url(schema, variant)));
    annotated.insert("title".into(), json!("SpecForge Graph Protocol"));
    annotated.insert(
        "description".into(),
        json!(format!(
            "Graph Protocol schema v{version}, {what} export: auto-generated from extension registries"
        )),
    );
    annotated.extend(std::mem::take(root));
    Some(Value::Object(annotated))
}

/// The kinds in the order the schema lists them (declared kinds as the
/// schema has them), then the structural ones.
fn ordered<'s>(schema: &'s GraphProtocolSchema, mut kinds: Vec<&'s str>) -> Vec<&'s str> {
    let rank = |kind: &&str| {
        schema
            .entity_kinds
            .iter()
            .position(|declared| declared.name == *kind)
            .unwrap_or(usize::MAX)
    };
    kinds.sort_by_key(|kind| (rank(kind), *kind));
    kinds
}

/// `kind`'s `fields` object: each declared field's form, the required fields required, any other key a field value.
fn fields_schema(kind: &SchemaEntityKind, defs: &mut Definitions) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in &kind.fields {
        properties.insert(field.name.clone(), field.form().json_schema(defs));
        if field.required {
            required.push(json!(field.name));
        }
    }
    let mut object = Map::new();
    object.insert("type".into(), json!("object"));
    object.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        object.insert("required".into(), Value::Array(required));
    }
    object.insert("additionalProperties".into(), FieldJson::schema(defs));
    Value::Object(object)
}

/// The value at `pointer` of the export document.
fn at<'v>(document: &'v mut Value, pointer: &str) -> &'v mut Value {
    document
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("the export document holds {pointer}"))
}
