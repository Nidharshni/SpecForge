//! `#[derive(Shape)]`: the schema it derives is the shape serde writes
//! (ADR 0048). One probe type per rule, asserting the exact schema.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};
use specforge_common::shape::{Object, Shape, violations};
use specforge_common::{DiagnosticJson, DiagnosticList, SourceSpan};

fn schema<T: Shape>() -> Value {
    T::schema()
}

/// What `value` serializes as conforms to the derived schema.
fn accepts<T: Shape + Serialize>(value: &T) {
    let written = serde_json::to_value(value).expect("a value serializes");
    let found = violations(&T::schema(), &written);
    assert!(found.is_empty(), "{written} breaks its schema: {found:?}");
}

fn is_object<T: Object>() {}

#[derive(Serialize, Shape)]
struct Plain {
    name: String,
    count: usize,
    ratio: f64,
    flag: bool,
    tags: Vec<String>,
}

#[test]
fn a_struct_is_a_closed_object_in_field_order() {
    is_object::<Plain>();
    assert_eq!(
        schema::<Plain>(),
        json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "count": {"type": "integer", "minimum": 0},
                "ratio": {"type": "number"},
                "flag": {"type": "boolean"},
                "tags": {"type": "array", "items": {"type": "string"}},
            },
            "required": ["name", "count", "ratio", "flag", "tags"],
            "additionalProperties": false,
        })
    );
    accepts(&Plain {
        name: "a".into(),
        count: 1,
        ratio: 0.5,
        flag: true,
        tags: vec!["x".into()],
    });
}

#[derive(Serialize, Shape)]
struct Skipped {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

#[test]
fn an_option_skipped_when_none_is_not_required() {
    assert_eq!(
        schema::<Skipped>(),
        json!({
            "type": "object",
            "properties": {"title": {"type": "string"}},
            "additionalProperties": false,
        })
    );
    accepts(&Skipped { title: None });
    accepts(&Skipped {
        title: Some("t".into()),
    });
}

#[derive(Serialize, Shape)]
struct Nullable {
    title: Option<String>,
    span: Option<SourceSpan>,
}

#[test]
fn an_option_written_as_null_is_nullable() {
    let schema = schema::<Nullable>();
    assert_eq!(schema["required"], json!(["title", "span"]));
    assert_eq!(
        schema["properties"]["title"],
        json!({"type": ["string", "null"]})
    );
    assert_eq!(
        schema["properties"]["span"]["anyOf"][1],
        json!({"type": "null"})
    );
    accepts(&Nullable {
        title: None,
        span: None,
    });
}

#[derive(Serialize, Shape)]
struct Conditional {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    items: Vec<usize>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    flag: bool,
}

#[test]
fn a_skip_serializing_if_field_is_not_required() {
    assert_eq!(
        schema::<Conditional>(),
        json!({
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"type": "integer", "minimum": 0}},
                "flag": {"type": "boolean"},
            },
            "additionalProperties": false,
        })
    );
    accepts(&Conditional {
        items: vec![],
        flag: false,
    });
}

#[derive(Serialize, Shape)]
#[serde(rename_all = "camelCase")]
struct Renamed {
    first_name: String,
    #[serde(rename = "other-name")]
    second_name: String,
    #[serde(skip)]
    #[allow(dead_code)]
    hidden: String,
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    also_hidden: String,
}

#[test]
fn rename_and_rename_all_name_the_keys() {
    assert_eq!(
        schema::<Renamed>(),
        json!({
            "type": "object",
            "properties": {
                "firstName": {"type": "string"},
                "other-name": {"type": "string"},
            },
            "required": ["firstName", "other-name"],
            "additionalProperties": false,
        })
    );
    accepts(&Renamed {
        first_name: "a".into(),
        second_name: "b".into(),
        hidden: "c".into(),
        also_hidden: "d".into(),
    });
}

#[derive(Serialize, Shape)]
struct Inner {
    a: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    b: Option<usize>,
}

#[derive(Serialize, Shape)]
struct Outer {
    id: String,
    #[serde(flatten)]
    inner: Inner,
}

#[test]
fn a_flattened_struct_merges_its_properties() {
    assert_eq!(
        schema::<Outer>(),
        json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "a": {"type": "string"},
                "b": {"type": "integer", "minimum": 0},
            },
            "required": ["id", "a"],
            "additionalProperties": false,
        })
    );
    accepts(&Outer {
        id: "x".into(),
        inner: Inner {
            a: "y".into(),
            b: Some(1),
        },
    });
}

#[derive(Serialize, Shape)]
struct Open {
    id: String,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

#[derive(Serialize, Shape)]
struct Typed {
    id: String,
    #[serde(flatten)]
    rest: BTreeMap<String, usize>,
}

#[test]
fn a_flattened_map_opens_the_object() {
    assert_eq!(
        schema::<Open>(),
        json!({
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "required": ["id"],
        })
    );
    assert_eq!(
        schema::<Typed>()["additionalProperties"],
        json!({"type": "integer", "minimum": 0})
    );
}

#[derive(Serialize, Shape)]
#[serde(untagged)]
enum Either {
    Left { left: String },
    Right { right: usize },
}

#[derive(Serialize, Shape)]
struct Wrapper<R> {
    #[serde(flatten)]
    reply: R,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<Vec<String>>,
}

#[test]
fn a_flattened_union_distributes_over_its_branches() {
    let schema = schema::<Wrapper<Either>>();
    let branches = schema["oneOf"].as_array().expect("a union");
    assert_eq!(branches.len(), 2);
    for branch in branches {
        assert_eq!(
            branch["properties"]["extra"],
            json!({"type": "array", "items": {"type": "string"}})
        );
        assert!(branch["required"].as_array().is_some());
    }
    accepts(&Wrapper {
        reply: Either::Left { left: "l".into() },
        extra: Some(vec!["e".into()]),
    });
    accepts(&Wrapper {
        reply: Either::Right { right: 2 },
        extra: None,
    });
}

#[derive(Serialize, Shape)]
#[serde(rename_all = "snake_case")]
enum Kind {
    FirstKind,
    #[serde(rename = "other")]
    Second,
}

#[test]
fn a_unit_enum_is_its_names() {
    assert_eq!(
        schema::<Kind>(),
        json!({"type": "string", "enum": ["first_kind", "other"]})
    );
    accepts(&Kind::FirstKind);
    accepts(&Kind::Second);
}

#[derive(Serialize, Shape)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Tagged {
    Empty,
    Fields { value: String },
    Wrapped(Inner),
}

#[test]
fn a_tagged_enum_is_a_union_of_tagged_objects() {
    is_object::<Tagged>();
    let schema = schema::<Tagged>();
    assert_eq!(schema["type"], "object");
    let branches = schema["oneOf"].as_array().expect("a union");
    assert_eq!(branches.len(), 3);
    assert_eq!(
        branches[0]["properties"]["kind"],
        json!({"type": "string", "enum": ["empty"]})
    );
    assert_eq!(branches[1]["required"], json!(["kind", "value"]));
    assert_eq!(branches[2]["required"], json!(["kind", "a"]));
    accepts(&Tagged::Empty);
    accepts(&Tagged::Fields { value: "v".into() });
    accepts(&Tagged::Wrapped(Inner {
        a: "a".into(),
        b: None,
    }));
}

#[test]
fn an_untagged_enum_is_a_union_of_its_variants() {
    is_object::<Either>();
    let schema = schema::<Either>();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["oneOf"].as_array().map(Vec::len), Some(2));
    accepts(&Either::Left { left: "l".into() });
    accepts(&Either::Right { right: 1 });
}

struct Table;

impl Table {
    fn names(&self) -> impl Iterator<Item = &'static str> {
        ["one", "two"].into_iter()
    }
}

const TABLE: Table = Table;

fn text<S: serde::Serializer>(_: &usize, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str("text")
}

#[derive(Serialize, Shape)]
struct Stated {
    #[shape(names = TABLE)]
    choice: String,
    #[shape(names = TABLE)]
    #[serde(skip_serializing_if = "Option::is_none")]
    maybe: Option<String>,
    #[shape(names = TABLE)]
    many: Vec<String>,
    #[serde(serialize_with = "text")]
    #[shape(as = String)]
    number: usize,
}

#[test]
fn shape_as_and_shape_names_state_the_field() {
    let schema = schema::<Stated>();
    let one_of = json!({"type": "string", "enum": ["one", "two"]});
    assert_eq!(schema["properties"]["choice"], one_of);
    assert_eq!(schema["properties"]["maybe"], one_of);
    assert_eq!(
        schema["properties"]["many"],
        json!({"type": "array", "items": one_of})
    );
    assert_eq!(schema["properties"]["number"], json!({"type": "string"}));
    assert_eq!(schema["required"], json!(["choice", "many", "number"]));
}

#[derive(Serialize, Shape)]
struct Generic<T> {
    item: T,
    items: Vec<T>,
}

#[test]
fn generic_parameters_are_shapes() {
    assert_eq!(
        schema::<Generic<bool>>()["properties"]["item"],
        json!({"type": "boolean"})
    );
    assert_eq!(
        schema::<Generic<Plain>>()["properties"]["items"]["items"]["type"],
        "object"
    );
}

#[test]
fn unsigned_integers_state_minimum_zero() {
    assert_eq!(schema::<u64>(), json!({"type": "integer", "minimum": 0}));
    assert_eq!(schema::<i32>(), json!({"type": "integer"}));
    assert_eq!(schema::<f32>(), json!({"type": "number"}));
    assert_eq!(schema::<Value>(), json!({}));
}

#[test]
fn the_diagnostics_json_shape() {
    let schema = schema::<DiagnosticJson<'static>>();
    assert_eq!(
        schema["required"],
        json!([
            "code",
            "title",
            "severity",
            "message",
            "span",
            "suggestion",
            "file",
            "line",
            "column"
        ])
    );
    assert_eq!(
        schema["properties"]["severity"],
        json!({"type": "string", "enum": ["Error", "Warning", "Info"]})
    );
    assert_eq!(
        schema["properties"]["title"],
        json!({"type": ["string", "null"]})
    );
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"]["data"]["oneOf"].is_array());
    assert!(schema["properties"]["origin"].is_object());
    assert_eq!(
        schema["properties"]["line"]["type"],
        json!(["integer", "null"])
    );
}

#[test]
fn every_derived_schema_accepts_what_serde_writes() {
    use specforge_common::{Diagnostic, DiagnosticData, codes};
    let bare = Diagnostic::new(codes::E003, "unresolved");
    let spanned = Diagnostic::new(codes::E003, "unresolved")
        .with_span(SourceSpan {
            file: "a.spec".into(),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 4,
        })
        .with_suggestion("did you mean b?");
    let with_data =
        Diagnostic::new(codes::E003, "unresolved").with_data(DiagnosticData::UnresolvedReference {
            target: "ghost".into(),
            entity: "alpha".into(),
            field: "refines".into(),
            did_you_mean: None,
        });
    for list in [
        DiagnosticList(vec![]),
        DiagnosticList(vec![bare.clone()]),
        DiagnosticList(vec![bare, spanned, with_data]),
    ] {
        accepts(&list);
    }
}

#[test]
fn additional_properties_false_refuses_an_undeclared_key() {
    let closed = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}},
        "additionalProperties": false,
    });
    assert!(violations(&closed, &json!({"a": "x"})).is_empty());
    assert_eq!(
        violations(&closed, &json!({"a": "x", "extra": 1})),
        ["$.extra: undeclared key"]
    );
    // Open when it says nothing.
    let open = json!({"type": "object", "properties": {"a": {"type": "string"}}});
    assert!(violations(&open, &json!({"a": "x", "extra": 1})).is_empty());
}

#[test]
fn additional_properties_schema_checks_each_extra_value() {
    let map = json!({
        "type": "object",
        "properties": {"fixed": {"type": "boolean"}},
        "additionalProperties": {"type": "integer"},
    });
    assert!(violations(&map, &json!({"fixed": true, "a": 1, "b": 2})).is_empty());
    assert_eq!(
        violations(&map, &json!({"fixed": true, "a": "one"})),
        ["$.a: expected integer, got string"]
    );
}

#[test]
fn minimum_refuses_a_smaller_number() {
    let schema = json!({"type": "integer", "minimum": 5});
    assert!(violations(&schema, &json!(5)).is_empty());
    assert_eq!(
        violations(&schema, &json!(3)),
        ["$: 3 is below the minimum 5"]
    );
    let nested = json!({"type": "object", "properties": {"n": {"type": "integer", "minimum": 5}}});
    assert_eq!(
        violations(&nested, &json!({"n": 3})),
        ["$.n: 3 is below the minimum 5"]
    );
}

#[derive(Serialize, Shape)]
struct Left {
    left: String,
}

#[derive(Serialize, Shape)]
struct Right {
    right: usize,
}

#[derive(Serialize, Shape)]
#[serde(untagged)]
enum Wrapped {
    A(Left),
    B(Right),
}

#[test]
fn an_untagged_union_of_object_newtypes_is_an_object() {
    is_object::<Wrapped>();
    assert_eq!(schema::<Wrapped>()["type"], "object");
    accepts(&Wrapped::A(Left { left: "l".into() }));
    accepts(&Wrapped::B(Right { right: 1 }));
}

/// The derive only unions `Object`s, so a scalar branch is reached through
/// `one_of` directly: a union is typed `object` only when every branch is.
#[test]
fn an_untagged_union_of_a_scalar_newtype_states_no_type() {
    let mixed = specforge_common::shape::one_of(vec![schema::<String>(), schema::<Left>()]);
    assert_eq!(mixed.get("type"), None);
    assert_eq!(mixed["oneOf"].as_array().map(Vec::len), Some(2));
}
