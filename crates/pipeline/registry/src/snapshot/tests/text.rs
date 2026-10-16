use std::path::Path;

use specforge_graph::ValueShape;
use specforge_parser::FieldValue;
use specforge_test_macros::test as specforge_test;

use crate::compilation::RegistryBuild;
use crate::snapshot::{EntitySnapshot, field_items, field_text};

/// Every field of `source`'s first entity: key, variant name, text.
fn texts(source: &str) -> Vec<(String, &'static str, String)> {
    let parsed = specforge_parser::parse(source, "t.spec");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.entities[0]
        .fields
        .entries()
        .iter()
        .map(|e| {
            let variant = match &e.value {
                FieldValue::String(_) => "String",
                FieldValue::Identifier(_) => "Identifier",
                FieldValue::Date(_) => "Date",
                FieldValue::Integer(_) => "Integer",
                FieldValue::Boolean(_) => "Boolean",
                FieldValue::List(_) => "List",
                FieldValue::VariantList(_) => "VariantList",
                FieldValue::TypeUnion(_) => "TypeUnion",
                FieldValue::Expression(_) => "Expression",
                FieldValue::Block(_) => "Block",
            };
            // The text is the items joined, for every list-shaped value.
            if let Some(items) = field_items(&e.value) {
                let joiner = match variant {
                    "VariantList" | "TypeUnion" => " | ",
                    _ => ", ",
                };
                assert_eq!(items.join(joiner), field_text(&e.value), "{}", e.key);
            }
            (e.key.to_string(), variant, field_text(&e.value))
        })
        .collect()
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "every written field keeps its value shape and value span beside its text"
)]
fn every_variant_has_a_value_shape() {
    let source = r#"item x "X" {
  title "a title"
  owner alice
  due 2026-10-06
  count 42
  active true
  labels ["a, b", "c"]
  needs [y, z]
  values [low, high]
  shape string | string[]
  mix [1, true, two]
  metric expr { latency < 10ms, load > 5 }
  ensures {
    done "it is done"
  }
  verify unit "x works"
}
"#;
    let registries = RegistryBuild::default();
    let graph = super::graph_of(source, &registries);
    let snapshot = EntitySnapshot::of(&graph, &registries, Path::new("."));
    let record = snapshot.records().iter().find(|r| r.id == "x").unwrap();
    let shapes: Vec<(&str, ValueShape)> = record
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f.shape))
        .collect();
    assert_eq!(
        shapes,
        [
            ("title", ValueShape::String),
            ("owner", ValueShape::Identifier),
            ("due", ValueShape::Date),
            ("count", ValueShape::Integer),
            ("active", ValueShape::Boolean),
            ("labels", ValueShape::List),
            ("needs", ValueShape::List),
            ("values", ValueShape::List),
            ("shape", ValueShape::TypeUnion),
            ("mix", ValueShape::List),
            ("metric", ValueShape::Expression),
            ("ensures", ValueShape::Block),
        ]
    );
    for shape in [ValueShape::List, ValueShape::Variants] {
        assert!(shape.is_list());
    }
    assert!(!ValueShape::Block.is_list());

    // Each value carries the span it is written at: its own line.
    let lines: Vec<(&str, Option<usize>)> = record
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f.value_span.as_ref().map(|s| s.start_line)))
        .collect();
    for (key, line) in [("title", 2), ("count", 5), ("labels", 7), ("needs", 8)] {
        assert_eq!(
            lines.iter().find(|(k, _)| *k == key).map(|(_, l)| *l),
            Some(Some(line)),
            "{key}: {lines:?}"
        );
    }
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "a variant list or type union is its members joined by ' | ', a list or expression group its items joined by ', '"
)]
fn field_text_of_every_variant() {
    let source = r#"item x "X" {
  title "a title"
  owner alice
  due 2026-10-06
  count 42
  active true
  labels ["a, b", "c"]
  needs [y, z]
  values [low, high]
  shape string | string[]
  mix [1, true, two]
  metric expr { latency < 10ms, load > 5 }
  ensures {
    done "it is done"
    kept "it is kept"
  }
  verify unit "x works"
  verify "x holds"
}
"#;
    let expected = [
        ("title", "String", "a title"),
        ("owner", "Identifier", "alice"),
        ("due", "Date", "2026-10-06"),
        ("count", "Integer", "42"),
        ("active", "Boolean", "true"),
        ("labels", "List", "a, b, c"),
        ("needs", "List", "y, z"),
        ("values", "List", "low, high"),
        ("shape", "TypeUnion", "string | string[]"),
        ("mix", "List", "1, true, two"),
        ("metric", "Expression", "latency < 10ms, load > 5"),
        ("ensures", "Block", "done, kept"),
    ];
    let actual = texts(source);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(k, v, t)| (k.to_string(), *v, t.to_string()))
            .collect::<Vec<_>>()
    );

    // A list keeps its items unjoined: the joined text of `labels`
    // cannot tell "a, b" from "a" and "b"; its items can.
    let parsed = specforge_parser::parse(source, "t.spec");
    let items: Vec<(String, Option<Vec<String>>)> = parsed.entities[0]
        .fields
        .entries()
        .iter()
        .map(|e| (e.key.to_string(), field_items(&e.value)))
        .collect();
    let of = |key: &str| {
        items
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, items)| items.clone())
    };
    let owned = |list: &[&str]| Some(list.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(of("labels"), owned(&["a, b", "c"]));
    assert_eq!(of("values"), owned(&["low", "high"]));
    assert_eq!(of("shape"), owned(&["string", "string[]"]));
    assert_eq!(of("mix"), owned(&["1", "true", "two"]));
    assert_eq!(of("metric"), owned(&["latency < 10ms", "load > 5"]));
    assert_eq!(of("ensures"), owned(&["done", "kept"]));
    for scalar in ["title", "owner", "due", "count", "active"] {
        assert_eq!(of(scalar), None, "{scalar}");
    }
}

#[specforge_test(
    behavior = "snapshot_entities_once",
    verify = "an empty list or block is written, with empty text, never left out or null"
)]
fn an_empty_value_has_empty_text() {
    let actual = texts("item x \"X\" {\n  values []\n  tags []\n  requires {\n  }\n}\n");
    assert_eq!(
        actual,
        [
            ("values".to_string(), "List", String::new()),
            ("tags".to_string(), "List", String::new()),
            ("requires".to_string(), "Block", String::new()),
        ]
    );
}
