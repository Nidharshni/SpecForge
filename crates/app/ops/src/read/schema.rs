//! `specforge schema` and `specforge.schema`: the versioned Graph Protocol
//! schema, one operation over the project view (ADR 0015). Both surfaces
//! answer one document for one request: the operation answers the document
//! itself (ADR 0052, round-18 amendment). `specforge schema
//! --publish` is [`json_schema`].

use serde::Serialize;
use serde_json::Value;
use specforge_common::shape::Shape;
use specforge_emitter::{SchemaEdgeType, SchemaEntityKind, SchemaExtensionInfo, SchemaVersion};

use crate::read::export::{self, AGENT_FORMAT, FORMAT};
use crate::view::ProjectView;
use crate::{Noticed, OpError, Refusal};

/// What part of the schema to return.
#[derive(Debug, Clone, Copy)]
pub struct SchemaRequest<'r> {
    /// That kind and the edge types that can start or end at it.
    pub kind: Option<&'r str>,
    /// Include the edge types.
    pub edges: bool,
    /// Add the validation rules the loaded extensions declare.
    pub validation_rules: bool,
}

impl Default for SchemaRequest<'_> {
    fn default() -> Self {
        SchemaRequest {
            kind: None,
            edges: true,
            validation_rules: false,
        }
    }
}

/// What `specforge schema` and `specforge.schema` answer with: the schema
/// as one document, its keys in `GraphProtocolSchema` order
/// (`schema_version`, `extensions`, `entity_kinds`), then `edge_types` when
/// edges were asked for and `validation_rules` when they were. Unfiltered,
/// it is the schema's own serialization.
#[derive(Debug, Clone, Serialize, Shape)]
pub struct SchemaDocument {
    pub schema_version: SchemaVersion,
    pub extensions: Vec<SchemaExtensionInfo>,
    pub entity_kinds: Vec<SchemaEntityKind>,
    /// Absent when edges were not asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_types: Option<Vec<SchemaEdgeType>>,
    /// Each loaded extension's rules: open values, the extension's own.
    /// Absent unless asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_rules: Option<Vec<Value>>,
}

/// The standalone JSON Schema (draft 2020-12) a `format` export of the
/// view's project conforms to, built from the versioned schema (`specforge
/// schema --publish`), with the notices of the versioned schema it is built
/// from (ADR 0111). `dot` has none: refused as [`AGENT_FORMAT`] refuses it.
pub fn json_schema(view: &ProjectView, format: export::Format) -> Result<Noticed<Value>, OpError> {
    if !AGENT_FORMAT.admits(format) {
        return Err(AGENT_FORMAT
            .parse(FORMAT.name_of(format))
            .expect_err("a format the agent table does not admit is refused"));
    }
    let versioned = view.versioned_schema();
    Ok(Noticed::new(
        specforge_emitter::publish(versioned.schema(), format.emit_format())
            .expect("an agent format has a schema"),
        versioned.notices(),
    ))
}

/// The field of a schema request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaField {
    /// `SchemaRequest::kind`.
    Kind,
}

/// The view's versioned schema, as `request` selects it. A kind no loaded
/// extension declares is `unknown_kind` ([`ProjectView::kinds`]), naming the
/// closest one.
pub fn schema(
    view: &ProjectView,
    request: &SchemaRequest,
) -> Result<Noticed<SchemaDocument>, Refusal<SchemaField>> {
    let versioned = view.versioned_schema();
    let notices = versioned.notices();
    let mut schema = versioned.into_schema();
    if let Some(kind) = request.kind {
        view.kinds()
            .declared(kind)
            .map_err(|error| error.about(SchemaField::Kind))?;
        schema.entity_kinds.retain(|entry| entry.name == kind);
        schema.edge_types.retain(|edge| touches(edge, kind));
    }
    let validation_rules = request.validation_rules.then(|| {
        view.registries()
            .declarations()
            .iter()
            .flat_map(|declaration| {
                declaration.validation_rules.iter().map(|rule| {
                    let mut rule = rule_json(rule);
                    rule["extension"] = Value::from(declaration.name());
                    rule
                })
            })
            .filter(|rule| {
                request
                    .kind
                    .is_none_or(|kind| rule["targetKind"].as_str() == Some(kind))
            })
            .collect()
    });
    Ok(Noticed::new(
        SchemaDocument {
            schema_version: schema.schema_version,
            extensions: schema.extensions,
            entity_kinds: schema.entity_kinds,
            edge_types: request.edges.then_some(schema.edge_types),
            validation_rules,
        },
        notices,
    ))
}

/// Whether an edge type can start or end at `kind`. An edge that names no
/// kinds on a side is open on that side.
fn touches(edge: &SchemaEdgeType, kind: &str) -> bool {
    let on =
        |kinds: &Option<Vec<String>>| kinds.as_ref().is_none_or(|k| k.iter().any(|k| k == kind));
    on(&edge.source_kinds) || on(&edge.target_kinds)
}

/// A declared validation rule as the schema lists it: its descriptor, keys
/// in camelCase (`message_template` is `messageTemplate`).
fn rule_json(rule: &specforge_protocol_types::ValidationRuleDescriptor) -> Value {
    let Ok(Value::Object(fields)) = serde_json::to_value(rule) else {
        unreachable!("a validation rule descriptor serializes to an object")
    };
    fields
        .into_iter()
        .map(|(key, value)| (camel_case(&key), value))
        .collect()
}

/// `snake_case` as `camelCase`.
fn camel_case(key: &str) -> String {
    let mut words = key.split('_');
    let mut out = words.next().unwrap_or_default().to_string();
    for word in words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_protocol_types::ValidationRuleDescriptor;

    #[test]
    fn a_rule_is_listed_with_camel_case_keys() {
        let rule = ValidationRuleDescriptor {
            code: "W901".to_string(),
            message_template: "{id} has no owner".to_string(),
            check: "field_required".to_string(),
            target_kind: Some("memo".to_string()),
            ..Default::default()
        };
        let json = rule_json(&rule);
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "check",
                "code",
                "constraint",
                "edgeType",
                "field",
                "messageTemplate",
                "severity",
                "targetKind",
                "wasmFunction"
            ]
        );
        assert_eq!(json["targetKind"], "memo");
        assert_eq!(json["messageTemplate"], "{id} has no owner");
        assert_eq!(camel_case("wasm_function"), "wasmFunction");
        assert_eq!(camel_case("code"), "code");
    }
}
