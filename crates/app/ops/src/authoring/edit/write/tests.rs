use super::*;
use specforge_graph::FieldForm;
use std::collections::BTreeMap;

fn form(field_type: FieldType) -> FieldForm {
    FieldForm::new(field_type, vec!["done".to_string()])
}

/// `written` for a key at two spaces, under the default configuration.
fn at_two(reading: Reading<'_>, key: &str, input: &Input) -> Result<String, Refusal<EditField>> {
    written(reading, key, input, "  ", &FormatConfig::default())
}

fn declared(field_type: FieldType, key: &str, input: Input) -> String {
    at_two(Reading::Declared(&form(field_type)), key, &input).unwrap()
}

fn one(text: &str) -> Input {
    Input::One(text.to_string())
}

fn many(items: &[&str]) -> Input {
    Input::Many(items.iter().map(|i| i.to_string()).collect())
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "set writes the value in the form its field declares"
)]
fn each_field_type_is_written_in_its_form() {
    assert_eq!(declared(FieldType::String, "note", one("hi")), r#""hi""#);
    assert_eq!(declared(FieldType::Integer, "priority", one("42")), "42");
    assert_eq!(
        declared(FieldType::Integer, "priority", one("many")),
        r#""many""#
    );
    assert_eq!(declared(FieldType::Bool, "abstract", one("true")), "true");
    assert_eq!(declared(FieldType::Enum, "status", one("done")), "done");
    assert_eq!(
        declared(FieldType::Enum, "status", one("two words")),
        r#""two words""#
    );
    assert_eq!(
        declared(FieldType::Identifier, "category", one("command")),
        "command"
    );
    assert_eq!(
        declared(FieldType::Reference, "refines", one("alpha")),
        "alpha"
    );
    assert_eq!(
        declared(
            FieldType::ReferenceList,
            "features",
            many(&["a_one", "b_two"])
        ),
        "[a_one, b_two]"
    );
    assert_eq!(
        declared(FieldType::ReferenceList, "features", one("a_one, b_two")),
        "[a_one, b_two]"
    );
    assert_eq!(
        declared(
            FieldType::StringList,
            "refs",
            many(&["RFC 1", "gh.issue:7"])
        ),
        r#"["RFC 1", gh.issue:7]"#
    );
    let entries = Input::Entries(BTreeMap::from([(
        "ready".to_string(),
        "Graph built".to_string(),
    )]));
    assert_eq!(
        declared(FieldType::Block, "requires", entries),
        "{\n    ready \"Graph built\"\n  }"
    );
}

#[test]
fn a_string_past_max_width_is_triple_quoted() {
    let long = "word ".repeat(30);
    let long = long.trim_end();
    let written = declared(FieldType::String, "contract", one(long));
    assert_eq!(written, format!("\"\"\"\n    {long}\n  \"\"\""));
}

#[test]
fn a_string_with_a_line_break_is_triple_quoted() {
    let written = declared(FieldType::String, "contract", one("first\nsecond\n\nthird"));
    assert_eq!(
        written,
        "\"\"\"\n    first\n    second\n\n    third\n  \"\"\""
    );
}

#[test]
fn a_string_that_cannot_round_trip_triple_quoted_is_escaped() {
    let long = format!("{} tail", "x".repeat(100));
    for text in [
        "",
        "a\"b",
        "back\\slash",
        "two\nlines",
        "  leading",
        "has \"\"\" inside",
        "é→ unicode",
        long.as_str(),
        "trailing\n",
    ] {
        let written = declared(FieldType::String, "note", one(text));
        let source = format!("probe probe_entity \"T\" {{\n  note {written}\n}}\n");
        let file = specforge_parser::parse(&source, "p.spec");
        assert!(file.errors.is_empty(), "{text:?}: {written}");
        let entry = &file.entities[0].fields.entries()[0];
        assert_eq!(
            entry.value,
            FieldValue::String(text.to_string()),
            "{text:?}: {written}"
        );
    }
}

#[test]
fn a_reference_spelled_true_or_false_is_quoted() {
    assert_eq!(
        declared(FieldType::Reference, "refines", one("true")),
        r#""true""#
    );
    assert_eq!(
        declared(
            FieldType::ReferenceList,
            "features",
            many(&["a_one", "false"])
        ),
        r#"[a_one, "false"]"#
    );
}

#[test]
fn a_member_type_expression_is_written_as_given() {
    for ty in ["string", "Task[]", "A | B"] {
        let written = at_two(Reading::Member, "taskId", &one(ty)).unwrap();
        assert_eq!(written, ty);
    }
}

#[test]
fn a_member_with_spaces_is_quoted() {
    assert_eq!(
        at_two(Reading::Member, "taskId", &one("two words {")).unwrap(),
        r#""two words {""#
    );
}

#[test]
fn a_value_that_does_not_fit_the_field_is_refused() {
    let refusal = at_two(
        Reading::Declared(&form(FieldType::String)),
        "note",
        &many(&["a", "b"]),
    )
    .unwrap_err();
    assert_eq!(refusal.error.code, NOT_EDITABLE);
    assert!(at_two(Reading::Undeclared, "x", &one("y")).is_err());
    assert!(at_two(Reading::Declared(&form(FieldType::Block)), "b", &one("y")).is_err());
}

#[test]
fn a_string_is_laid_out_with_the_configuration() {
    let long = "Given valid credentials the behavior returns an auth token that the client stores and sends with every later request it makes";
    let tabs = FormatConfig {
        use_tabs: true,
        ..Default::default()
    };
    assert_eq!(
        string("contract", long, "\t", &tabs),
        format!("\"\"\"\n\t\t{long}\n\t\"\"\"")
    );
    let wide = FormatConfig {
        max_width: 200,
        ..Default::default()
    };
    assert_eq!(string("contract", long, "  ", &wide), spell::quoted(long));
}
