//! One module spells a value (ADR 0153): the escape table, the bare rule and each declared type's one-line form.

use std::collections::{BTreeSet, HashMap};

use specforge_graph::field_form::{FieldForm, FieldForms, spell};
use specforge_graph::{FieldValue, Written};
use specforge_protocol_types::FieldType;
use specforge_test_macros::test as specforge_test;

/// `text` parsed as the one field `k` of an entity.
fn field_of(source: &str) -> specforge_parser::SpecFile {
    let file = specforge_parser::parse(source, "p.spec");
    assert!(file.errors.is_empty(), "{source}: {:?}", file.errors);
    file
}

#[test]
fn a_quoted_text_reads_back() {
    for text in [
        "",
        "a\"b",
        "back\\slash",
        "two\nlines",
        "  leading",
        "has \"\"\" inside",
        "é→ unicode",
        "tab\there",
        "cr\rhere",
        "trailing\n",
    ] {
        let quoted = spell::quoted(text);
        assert!(!quoted.contains('\n'), "{text:?}");
        let file = field_of(&format!("probe p \"T\" {{\n  k {quoted}\n}}\n"));
        let [entity] = file.entities.as_slice() else {
            panic!("{text:?}: {quoted}");
        };
        let [entry] = entity.fields.entries() else {
            panic!("{text:?}: {quoted}");
        };
        assert_eq!(
            entry.value,
            FieldValue::String(text.to_string()),
            "{text:?}: {quoted}"
        );
    }
}

#[test]
fn each_declared_type_spells_its_value() {
    for (field_type, text, expected) in [
        (FieldType::String, "hi", r#""hi""#),
        (FieldType::Identifier, "command", "command"),
        (FieldType::Identifier, "api/lsp", r#""api/lsp""#),
        (FieldType::Enum, "true", r#""true""#),
        (FieldType::Reference, "alpha", "alpha"),
        (FieldType::Reference, "gh.issue:7", "gh.issue:7"),
        (FieldType::Reference, "true", r#""true""#),
        (FieldType::ReferenceList, "a_one", "a_one"),
        (FieldType::StringList, "RFC 1", r#""RFC 1""#),
        (FieldType::StringList, "gh.issue:7", "gh.issue:7"),
        (FieldType::Integer, " 42 ", "42"),
        (FieldType::Integer, "many", r#""many""#),
        (FieldType::Bool, "true", "true"),
        (FieldType::Bool, "yes", r#""yes""#),
        (FieldType::Block, "x", r#""x""#),
    ] {
        let form = FieldForm::new(field_type, vec![]);
        assert_eq!(
            spell::value(&form, text),
            expected,
            "{field_type:?} {text:?}"
        );
    }
}

#[specforge_test(
    invariant = "typed_edit_canonical_form",
    verify = "a spelled value reads back and is left as it is by format and W159"
)]
fn a_spelled_value_reads_back_and_is_left_by_rewrite() {
    let types = [
        FieldType::String,
        FieldType::Identifier,
        FieldType::Enum,
        FieldType::Integer,
        FieldType::Bool,
        FieldType::Reference,
        FieldType::ReferenceList,
        FieldType::StringList,
    ];
    let mut declared = HashMap::new();
    for (i, field_type) in types.iter().enumerate() {
        declared.insert(
            ("k".to_string(), format!("f{i}")),
            FieldForm::new(*field_type, vec![]),
        );
    }
    let forms = FieldForms::new(declared, BTreeSet::new(), BTreeSet::new());

    for (i, field_type) in types.iter().enumerate() {
        let form = FieldForm::new(*field_type, vec![]);
        let is_list = matches!(field_type, FieldType::ReferenceList | FieldType::StringList);
        for text in [
            "command",
            "true",
            "two words",
            "gh.issue:7",
            "a\"b",
            "",
            "42",
            "x-y",
            "é",
        ] {
            // A scheme ref id stands bare only as a list item; as a scalar it does not parse, and the
            // typed-edit writer refuses it at its own read-back.
            if !is_list && text == "gh.issue:7" && *field_type == FieldType::Reference {
                continue;
            }
            let spelled = spell::value(&form, text);
            let value = if is_list {
                format!("[{spelled}]")
            } else {
                spelled.clone()
            };
            let file = field_of(&format!("k k1 \"T\" {{\n  f{i} {value}\n}}\n"));
            let entry = &file.entities[0].fields.entries()[0];
            let span = entry.value_span.clone().unwrap();
            assert!(
                forms.rewrites("k", entry, &span).is_empty(),
                "{field_type:?} {text:?}: {value}"
            );
            // A non-integer on `integer` and a non-literal on `bool` are stored as text, for E061 to report.
            let by_design = (*field_type == FieldType::Integer && text.parse::<i64>().is_err())
                || (*field_type == FieldType::Bool && !matches!(text, "true" | "false"));
            if by_design {
                continue;
            }
            let stored = form
                .coerce(&entry.value, None)
                .unwrap_or(entry.value.clone());
            let read = match &stored {
                FieldValue::String(s) | FieldValue::Identifier(s) => s.clone(),
                FieldValue::Integer(n) => n.to_string(),
                FieldValue::Boolean(b) => b.to_string(),
                FieldValue::List(items) => {
                    assert_eq!(items.len(), 1, "{field_type:?} {text:?}: {value}");
                    items[0].text.clone()
                }
                other => panic!("{field_type:?} {text:?}: {value}: {other:?}"),
            };
            assert_eq!(read, text, "{field_type:?} {text:?}: {value}");
        }
    }
}

#[test]
fn spelled_writes_the_given_form() {
    assert_eq!(spell::spelled("a\"b", Written::Quoted), r#""a\"b""#);
    assert_eq!(spell::spelled("x", Written::Bare), "x");
    assert_eq!(
        spell::spelled("gh.issue:1", Written::SchemeRef),
        "gh.issue:1"
    );
    assert_eq!(spell::spelled("7", Written::Literal), "7");
}
