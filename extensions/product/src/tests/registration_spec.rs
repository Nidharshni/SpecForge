//! `spec/extensions/product/behaviors-registration.spec` lists the edge
//! types and the fields each kind registers, as prose strings. These tests
//! hold those lists to the live declaration, both ways, so a change to
//! either side is noticed.

use specforge_protocol_types::FieldDescriptor;
use specforge_test::prelude::*;
use std::collections::BTreeMap;

const SPEC: &str = include_str!("../../../../spec/extensions/product/behaviors-registration.spec");

/// The lines of the `behavior <name>` block, up to its closing brace.
fn block(name: &str) -> Vec<&'static str> {
    let header = format!("behavior {name} ");
    SPEC.lines()
        .skip_while(|line| !line.starts_with(&header))
        .skip(1)
        .take_while(|line| line.trim_end() != "}")
        .collect()
}

/// The text between the first pair of double quotes of `line`.
fn quoted(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = line.rfind('"')?;
    (end > start).then(|| &line[start..end])
}

/// One field as the spec lists it: `name(type[->target][, required]
/// [, default <value>][, Edge])`.
#[derive(Debug, PartialEq, Eq)]
struct Listed {
    field_type: String,
    enumerated: bool,
    target_kind: Option<String>,
    required: bool,
    default_value: Option<String>,
    edge: Option<String>,
}

impl Listed {
    fn parse(args: &str) -> Listed {
        let mut parts = args.split(", ");
        let kind = parts.next().expect("a field lists its type first");
        let (field_type, enumerated, target_kind) = match kind.strip_prefix("enum ") {
            Some(_) => ("enum".to_string(), true, None),
            None => match kind.split_once("->") {
                Some((field_type, target)) => {
                    (field_type.to_string(), false, Some(target.to_string()))
                }
                None => (kind.to_string(), false, None),
            },
        };
        let mut listed = Listed {
            field_type,
            enumerated,
            target_kind,
            required: false,
            default_value: None,
            edge: None,
        };
        for part in parts {
            if part == "required" {
                listed.required = true;
            } else if let Some(value) = part.strip_prefix("default ") {
                listed.default_value = Some(value.to_string());
            } else {
                listed.edge = Some(part.to_string());
            }
        }
        listed
    }

    /// `declared` as the same listing.
    fn declared(declared: &FieldDescriptor) -> Listed {
        Listed {
            field_type: declared.field_type.clone(),
            enumerated: declared.field_type == "enum" && !declared.enum_values.is_empty(),
            target_kind: declared.target_kind.clone(),
            required: declared.required,
            default_value: declared.default_value.clone(),
            edge: declared.edge.clone(),
        }
    }
}

/// `<kind> has: a(...), b(...)` as name -> field.
fn listed_fields(list: &str) -> BTreeMap<String, Listed> {
    let mut items = list.split("), ").peekable();
    let mut fields = BTreeMap::new();
    while let Some(item) = items.next() {
        let item = if items.peek().is_some() {
            format!("{item})")
        } else {
            item.to_string()
        };
        let (name, args) = item.split_once('(').expect("a field is name(args)");
        let args = args.strip_suffix(')').expect("a field's args end with )");
        let listed = Listed::parse(args);
        assert!(
            fields.insert(name.to_string(), listed).is_none(),
            "{name} is listed twice in {list}"
        );
    }
    fields
}

#[specforge_test(
    behavior = "pe_register_field_definitions",
    verify = "the field lists this behavior ensures are the declaration's, field for field"
)]
fn the_field_lists_are_the_declarations() {
    let declaration = crate::specforge_extension_build().declaration();
    let lines = block("pe_register_field_definitions");

    let mut listed_kinds = Vec::new();
    for line in &lines {
        let Some(text) = quoted(line) else { continue };
        let Some((kind, list)) = text.split_once(" has: ") else {
            continue;
        };
        listed_kinds.push(kind.to_string());
        let kind_declared = declaration
            .entities
            .iter()
            .find(|entity| entity.keyword.as_deref() == Some(kind))
            .unwrap_or_else(|| panic!("the spec lists kind {kind}, the declaration declares none"));
        let listed = listed_fields(list);
        for (name, listing) in &listed {
            let field = kind_declared
                .fields
                .iter()
                .find(|field| &field.name == name)
                .unwrap_or_else(|| {
                    panic!("{kind}.{name}: the spec lists {listing:?}, the declaration has no such field")
                });
            assert_eq!(
                &Listed::declared(field),
                listing,
                "{kind}.{name}: declared (left) differs from the spec's list (right)"
            );
        }
        for field in &kind_declared.fields {
            assert!(
                listed.contains_key(&field.name),
                "{kind}.{}: declared as {:?}, absent from the spec's list",
                field.name,
                Listed::declared(field)
            );
        }
    }

    let mut declared_kinds: Vec<String> = declaration
        .entities
        .iter()
        .filter_map(|entity| entity.keyword.clone())
        .collect();
    declared_kinds.sort();
    listed_kinds.sort();
    assert_eq!(
        listed_kinds, declared_kinds,
        "the spec lists (left) and the declaration declares (right) different kinds"
    );

    // The shared field and the status defaults, which are not per-kind lines.
    let has = |key: &str| lines.iter().any(|line| line.trim_start().starts_with(key));
    assert!(
        has("shared_tags"),
        "the shared tags line is gone from the spec"
    );
    assert!(
        has("status_defaults"),
        "the status defaults line is gone from the spec"
    );
    let shared: Vec<&str> = declaration
        .shared_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(shared, ["tags"], "the spec says every kind also gets tags");
}

#[specforge_test(
    behavior = "pe_register_edge_types",
    verify = "the edge types this behavior ensures are the declaration's, with their kinds"
)]
fn the_edge_list_is_the_declarations() {
    let declaration = crate::specforge_extension_build().declaration();
    let mut listed = 0;
    for line in block("pe_register_edge_types") {
        let Some(text) = quoted(line) else { continue };
        let Some(edge) = text.strip_prefix("EdgeTypeSet contains ") else {
            continue;
        };
        let (label, kinds) = edge
            .split_once(" (")
            .expect("an edge lists (source->target)");
        let (source, target) = kinds
            .strip_suffix(')')
            .and_then(|kinds| kinds.split_once("->"))
            .expect("an edge lists source->target");
        let declared = declaration
            .edges
            .iter()
            .find(|declared| declared.label == label)
            .unwrap_or_else(|| panic!("{label}: listed by the spec, not declared"));
        assert_eq!(
            (
                declared.source_kind.as_deref(),
                declared.target_kind.as_deref()
            ),
            (Some(source), Some(target)),
            "{label}: declared kinds (left) differ from the spec's (right)"
        );
        listed += 1;
    }
    assert_eq!(
        listed,
        declaration.edges.len(),
        "the spec lists (left) a different number of edges than the declaration declares (right)"
    );
    assert_eq!(listed, 20, "twenty_edges_total claims 20 edges");
}
