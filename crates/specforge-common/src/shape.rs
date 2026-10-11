//! A type's JSON shape: the JSON Schema of what its `Serialize` writes (ADR
//! 0048).
//!
//! `#[derive(Shape)]` sits beside `#[derive(Serialize)]` and reads the same
//! fields and serde attributes, so the schema follows the serialization. A
//! hand-written `impl Shape` sits only beside a hand-written `Serialize` (a
//! scalar such as [`Sym`](crate::Sym)) or a value whose type is defined
//! elsewhere (`FieldType`'s names). The schemas are the subset
//! [`violations`] checks: `type`, `enum`, `properties`, `required`,
//! `additionalProperties`, `items`, `oneOf`, `anyOf`, `minimum`, and, for the
//! documents it checks, `$ref` to a definition of the same document. They
//! carry no `description`, `title` or `$schema`.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use serde_json::Value;
use serde_json::{Map, json};
pub use specforge_shape_macros::Shape;

/// The JSON Schema of the JSON a type serializes as.
pub trait Shape {
    /// An object is `{"type":"object","properties":…,"required":[…],
    /// "additionalProperties":false}` (`properties` keyed by the serialized
    /// names, `required` the keys always written); an array
    /// `{"type":"array","items":…}`; a closed set of names
    /// `{"type":"string","enum":[…]}`; a union `{"oneOf":[…]}` (with
    /// `"type":"object"` when every branch is one).
    fn schema() -> Value;
}

/// A type that serializes as a JSON object: what an MCP outputSchema's root
/// must be. The derive implements it for a struct with named fields, a
/// `#[serde(tag = …)]` enum and an untagged enum whose variants are objects.
pub trait Object: Shape {}

/// `schema` or `null`: `"type": [t, "null"]` for a plain scalar type, else
/// `{"anyOf": [schema, {"type": "null"}]}`. What an `Option` serialized
/// without `skip_serializing_if` is.
pub fn nullable(schema: Value) -> Value {
    let plain_scalar = matches!(
        schema.get("type").and_then(Value::as_str),
        Some("string" | "boolean" | "integer" | "number")
    ) && schema.get("enum").is_none()
        && schema.get("oneOf").is_none()
        && schema.get("anyOf").is_none();
    if plain_scalar {
        let mut schema = schema;
        let scalar = schema["type"].take();
        schema["type"] = json!([scalar, "null"]);
        return schema;
    }
    json!({ "anyOf": [schema, { "type": "null" }] })
}

/// `{"type": "string", "enum": names}`: a closed set of names, from the one
/// place that lists them (an option table's `names()`, ADR 0027).
pub fn names<'n>(names: impl IntoIterator<Item = &'n str>) -> Value {
    json!({ "type": "string", "enum": names.into_iter().collect::<Vec<_>>() })
}

/// `{"type": "array", "items": items}`.
pub fn array(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

/// A closed object: `properties` under their serialized names, `required`
/// the keys always written.
#[doc(hidden)]
pub fn object(properties: Vec<(&'static str, Value)>, required: Vec<&'static str>) -> Value {
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert(
        "properties".into(),
        Value::Object(
            properties
                .into_iter()
                .map(|(name, property)| (name.to_string(), property))
                .collect(),
        ),
    );
    if !required.is_empty() {
        schema.insert("required".into(), json!(required));
    }
    schema.insert("additionalProperties".into(), json!(false));
    Value::Object(schema)
}

/// `inner`'s fields are `into`'s own (`#[serde(flatten)]`): its properties
/// and required keys merge in; a map (an object open to any key) opens
/// `into` (to the map's value schema, or to any value); a union distributes
/// over its branches.
#[doc(hidden)]
pub fn flatten(into: &mut Value, inner: Value) {
    // `into` is already a union: every branch takes the field.
    if let Some(Value::Array(branches)) = into.get_mut("oneOf") {
        for branch in branches {
            flatten(branch, inner.clone());
        }
        return;
    }
    // `inner` is a union: `into` becomes one, each branch merged with
    // `into`'s own fields.
    if let Some(Value::Array(branches)) = inner.get("oneOf") {
        let merged = branches
            .iter()
            .map(|branch| {
                let mut object = into.clone();
                flatten(&mut object, branch.clone());
                object
            })
            .collect();
        *into = one_of(merged);
        return;
    }
    let (Some(target), Some(source)) = (into.as_object_mut(), inner.as_object()) else {
        return;
    };
    if let Some(properties) = source.get("properties").and_then(Value::as_object) {
        let entry = target
            .entry("properties")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(entry) = entry {
            entry.extend(properties.clone());
        }
    }
    if let Some(Value::Array(required)) = source.get("required") {
        let entry = target
            .entry("required")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(entry) = entry {
            entry.extend(required.clone());
        }
    }
    match source.get("additionalProperties") {
        // A map: the object takes any key (of the map's value schema).
        Some(Value::Bool(false)) | None if source.get("properties").is_some() => {}
        Some(Value::Object(values)) if !values.is_empty() => {
            target.insert("additionalProperties".into(), Value::Object(values.clone()));
        }
        _ => {
            target.remove("additionalProperties");
        }
    }
}

/// `{"oneOf": branches}`, typed `object` when every branch is one (decided
/// from the branches, not from the syntax of the type they came from).
#[doc(hidden)]
pub fn one_of(branches: Vec<Value>) -> Value {
    let objects = branches
        .iter()
        .all(|branch| branch.get("type") == Some(&json!("object")));
    if objects {
        json!({ "type": "object", "oneOf": branches })
    } else {
        json!({ "oneOf": branches })
    }
}

// ── the types serde_json and std define ─────────────────────────────────────

macro_rules! scalar {
    ($schema:expr => $($ty:ty),+ $(,)?) => {$(
        impl Shape for $ty {
            fn schema() -> Value {
                $schema
            }
        }
    )+};
}

scalar!(json!({"type": "string"}) => String, str, Cow<'_, str>, PathBuf, Path);
scalar!(json!({"type": "boolean"}) => bool);
scalar!(json!({"type": "integer", "minimum": 0}) => u8, u16, u32, u64, usize);
scalar!(json!({"type": "integer"}) => i8, i16, i32, i64, isize);
scalar!(json!({"type": "number"}) => f32, f64);

impl<T: Shape> Shape for Option<T> {
    fn schema() -> Value {
        nullable(T::schema())
    }
}

impl<T: Shape> Shape for Vec<T> {
    fn schema() -> Value {
        array(T::schema())
    }
}

impl<T: Shape> Shape for [T] {
    fn schema() -> Value {
        array(T::schema())
    }
}

impl<T: Shape> Shape for BTreeSet<T> {
    fn schema() -> Value {
        array(T::schema())
    }
}

impl<T: Shape> Shape for HashSet<T> {
    fn schema() -> Value {
        array(T::schema())
    }
}

/// A map: an object open to any key, its values of one schema.
fn map_of(values: Value) -> Value {
    match values.as_object() {
        Some(any) if any.is_empty() => json!({"type": "object"}),
        _ => json!({"type": "object", "additionalProperties": values}),
    }
}

impl<V: Shape> Shape for BTreeMap<String, V> {
    fn schema() -> Value {
        map_of(V::schema())
    }
}

impl<V: Shape> Shape for HashMap<String, V> {
    fn schema() -> Value {
        map_of(V::schema())
    }
}

impl Shape for Map<String, Value> {
    fn schema() -> Value {
        json!({"type": "object"})
    }
}

/// Any value: an open value (content an extension defines).
impl Shape for Value {
    fn schema() -> Value {
        json!({})
    }
}

impl<T: Shape + ?Sized> Shape for &T {
    fn schema() -> Value {
        T::schema()
    }
}

impl<T: Shape + ?Sized> Shape for Box<T> {
    fn schema() -> Value {
        T::schema()
    }
}

impl<T: Shape + ?Sized> Shape for Arc<T> {
    fn schema() -> Value {
        T::schema()
    }
}

impl<T: Object + ?Sized> Object for &T {}

impl<T: Object + ?Sized> Object for Box<T> {}

impl<T: Object + ?Sized> Object for Arc<T> {}

/// An interned symbol serializes as its string.
impl Shape for crate::Sym {
    fn schema() -> Value {
        json!({"type": "string"})
    }
}

/// A field type serializes as its wire name.
impl Shape for specforge_protocol_types::FieldType {
    fn schema() -> Value {
        names(Self::ALL.iter().map(|t| t.as_str()))
    }
}

// ── the check ───────────────────────────────────────────────────────────────

/// Where `value` breaks `document`, one message per violation, each naming
/// its path (`$.format`, `$.paths[0]`). Empty when it conforms. `document` is
/// the root a `$ref` resolves against: a same-document JSON Pointer
/// (`#/$defs/Diagnostic`, `#/definitions/X`, `#`) is followed; any other
/// reference (a URI) is never fetched and is a violation, as is one naming
/// nothing or a chain of references that never reaches a schema.
pub fn violations(document: &Value, value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    check(document, value, "$", document, 0, &mut found);
    found
}

/// The most references followed at one value before the chain is circular.
const MAX_HOPS: u8 = 32;

fn check(
    schema: &Value,
    value: &Value,
    path: &str,
    root: &Value,
    hops: u8,
    found: &mut Vec<String>,
) {
    if let Some(Value::String(reference)) = schema.get("$ref") {
        match reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
        {
            _ if hops >= MAX_HOPS => found.push(format!("{path}: circular $ref {reference}")),
            Some(target) => check(target, value, path, root, hops + 1, found),
            None => found.push(format!("{path}: unresolvable $ref {reference}")),
        }
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !types.is_empty() && !types.iter().any(|name| is_a(name, value)) {
        found.push(format!(
            "{path}: expected {}, got {}",
            types.join(" or "),
            type_name(value)
        ));
        return;
    }
    for (keyword, exactly_one) in [("oneOf", true), ("anyOf", false)] {
        let Some(Value::Array(branches)) = schema.get(keyword) else {
            continue;
        };
        let matching = branches
            .iter()
            .filter(|branch| {
                let mut branch_found = Vec::new();
                check(branch, value, path, root, hops, &mut branch_found);
                branch_found.is_empty()
            })
            .count();
        let conforms = if exactly_one {
            matching == 1
        } else {
            matching > 0
        };
        if !conforms {
            found.push(format!(
                "{path}: matches {matching} of the {} {keyword} schemas",
                branches.len()
            ));
        }
    }
    if let Some(Value::Array(allowed)) = schema.get("enum")
        && !allowed.contains(value)
    {
        found.push(format!(
            "{path}: {value} is not one of {}",
            Value::from(allowed.clone())
        ));
    }
    if let Value::Object(object) = value {
        for name in schema["required"].as_array().into_iter().flatten() {
            if let Some(name) = name.as_str()
                && !object.contains_key(name)
            {
                found.push(format!("{path}: missing required {name}"));
            }
        }
        let properties = schema["properties"].as_object();
        if let Some(properties) = properties {
            for (name, property) in properties {
                if let Some(member) = object.get(name) {
                    check(property, member, &format!("{path}.{name}"), root, 0, found);
                }
            }
        }
        // A key `properties` does not list: refused when the object is
        // closed, checked against the value schema when it is a map.
        match schema.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                for name in object.keys() {
                    if !properties.is_some_and(|properties| properties.contains_key(name)) {
                        found.push(format!("{path}.{name}: undeclared key"));
                    }
                }
            }
            Some(values @ Value::Object(_)) => {
                for (name, member) in object {
                    if !properties.is_some_and(|properties| properties.contains_key(name)) {
                        check(values, member, &format!("{path}.{name}"), root, 0, found);
                    }
                }
            }
            _ => {}
        }
    }
    if let (Some(minimum), Some(number)) = (
        schema.get("minimum").and_then(Value::as_f64),
        value.as_f64(),
    ) && number < minimum
    {
        found.push(format!(
            "{path}: {value} is below the minimum {}",
            schema["minimum"]
        ));
    }
    if let (Value::Array(items), Some(item)) = (value, schema.get("items")) {
        for (i, element) in items.iter().enumerate() {
            check(item, element, &format!("{path}[{i}]"), root, 0, found);
        }
    }
}

/// Whether `value` is of the JSON Schema type `name`; a name the subset
/// does not know accepts anything.
fn is_a(name: &str, value: &Value) -> bool {
    match name {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::violations;
    use serde_json::json;

    #[test]
    fn a_ref_is_resolved_within_the_document() {
        let schema = json!({"$defs": {"A": {"type": "string"}}, "$ref": "#/$defs/A"});
        assert_eq!(
            violations(&schema, &json!(1)),
            ["$: expected string, got integer"]
        );
        assert!(violations(&schema, &json!("x")).is_empty());
    }

    #[test]
    fn a_nested_ref_and_its_siblings_are_checked() {
        let schema = json!({
            "$defs": {
                "A": {"type": "object", "properties": {"b": {"$ref": "#/$defs/B"}}, "additionalProperties": false},
                "B": {"type": "string"}
            },
            "$ref": "#/$defs/A"
        });
        assert_eq!(
            violations(&schema, &json!({"b": 1})),
            ["$.b: expected string, got integer"]
        );
        assert_eq!(
            violations(&schema, &json!({"c": 1})),
            ["$.c: undeclared key"]
        );
        let sibling =
            json!({"$defs": {"A": {"type": "object"}}, "$ref": "#/$defs/A", "required": ["x"]});
        assert_eq!(violations(&sibling, &json!({})), ["$: missing required x"]);
    }

    #[test]
    fn an_unresolvable_ref_is_a_violation() {
        let schema = json!({"$ref": "#/$defs/Nope"});
        assert_eq!(
            violations(&schema, &json!(1)),
            ["$: unresolvable $ref #/$defs/Nope"]
        );
    }

    #[test]
    fn an_external_ref_is_never_fetched() {
        let schema = json!({"$ref": "https://example.com/s.json"});
        assert_eq!(
            violations(&schema, &json!(1)),
            ["$: unresolvable $ref https://example.com/s.json"]
        );
    }

    #[test]
    fn a_circular_ref_is_a_violation() {
        let schema = json!({"$defs": {"A": {"$ref": "#/$defs/A"}}, "$ref": "#/$defs/A"});
        let found = violations(&schema, &json!(1));
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("circular $ref"), "{found:?}");
    }

    #[test]
    fn draft_07_definitions_resolve() {
        let schema = json!({"definitions": {"X": {"type": "string"}}, "$ref": "#/definitions/X"});
        assert_eq!(
            violations(&schema, &json!(1)),
            ["$: expected string, got integer"]
        );
        let root = json!({"type": "object", "properties": {"a": {"$ref": "#"}}, "additionalProperties": false});
        assert!(violations(&root, &json!({"a": {"a": {}}})).is_empty());
    }

    #[test]
    fn one_of_needs_exactly_one_branch_and_any_of_at_least_one() {
        let branch = |key: &str| json!({"type": "object", "required": [key]});
        let one_of = json!({"type": "object", "oneOf": [branch("a"), branch("b")]});
        assert!(violations(&one_of, &json!({"a": 1})).is_empty());
        assert_eq!(
            violations(&one_of, &json!({"c": 1})),
            ["$: matches 0 of the 2 oneOf schemas"]
        );
        assert_eq!(
            violations(&one_of, &json!({"a": 1, "b": 2})),
            ["$: matches 2 of the 2 oneOf schemas"]
        );
        let any_of = json!({"anyOf": [branch("a"), branch("b")]});
        assert!(violations(&any_of, &json!({"a": 1, "b": 2})).is_empty());
        assert_eq!(
            violations(&any_of, &json!({})),
            ["$: matches 0 of the 2 anyOf schemas"]
        );
    }
}
