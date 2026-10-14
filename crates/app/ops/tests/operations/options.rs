//! The option tables (ADR 0027): each enumerated argument an operation
//! reads, its names, aliases and default, and how it refuses a name.

use specforge_emitter::model::{FieldLevel, GroupBy, ModelFormat, ModelOptions};
use specforge_emitter::outline::{DependencyDepth, OutlineDetail, OutlineFormat, OutlineOptions};
use specforge_ops::OpErrorKind;
use specforge_ops::export::{AGENT_FORMAT, FORMAT, Format};
use specforge_ops::model::{
    DEPS, GROUP_BY, MODEL_FIELDS, MODEL_FORMAT, OUTLINE_FIELDS, OUTLINE_FORMAT,
};
use specforge_ops::options::OptionTable;

#[specforge_test_macros::test(
    behavior = "name_enumerated_options_once",
    verify = "a table parses its names and aliases and refuses any other naming the expected names"
)]
fn a_table_parses_names_and_aliases_and_refuses_others() {
    assert_eq!(FORMAT.parse("json"), Ok(Format::Graph));
    assert_eq!(AGENT_FORMAT.parse("json"), Ok(Format::Graph));
    assert_eq!(MODEL_FORMAT.parse("dbml"), Ok(ModelFormat::Dbml));
    assert_eq!(DEPS.parse("full"), Ok(DependencyDepth::Full));

    let error = MODEL_FORMAT.parse("svg").unwrap_err();
    // One failure vocabulary: every table refuses as invalid input, a
    // format no differently from any other argument.
    assert_eq!(error.kind, OpErrorKind::InvalidInput);
    assert_eq!(error.code, "invalid_input");
    assert_eq!(
        error.message,
        "Unknown format: svg. Expected: markdown, mermaid, dot, json, dbml"
    );

    let error = GROUP_BY.parse("both").unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert_eq!(
        error.message,
        "Unknown group_by: both. Expected: extension, none"
    );

    let error = DEPS.parse("diret").unwrap_err();
    assert_eq!(
        error.message,
        "Unknown deps: diret. Expected: direct, effective, full"
    );
    assert_eq!(error.suggestion.as_deref(), Some("did you mean 'direct'?"));

    // An alias is accepted, never listed.
    let error = FORMAT.parse("yaml").unwrap_err();
    assert_eq!(
        error.message,
        "Unknown format: yaml. Expected: graph, context, brief, dot"
    );
    assert!(!FORMAT.names().any(|name| name == "json"));
    assert!(FORMAT.accepted().any(|name| name == "json"));

    // The agent formats are a subset of the export's.
    assert!(!AGENT_FORMAT.admits(Format::Dot));
    assert!(FORMAT.admits(Format::Dot));
    assert_eq!(
        AGENT_FORMAT.parse("dot").unwrap_err().message,
        "Unknown format: dot. Expected: graph, context, brief"
    );
}

/// A table's names and aliases are unique, each listed name names its
/// value back, and a table with a default parses absence as it.
fn check<T: Copy + PartialEq + std::fmt::Debug>(table: &OptionTable<T>) {
    let accepted: Vec<&str> = table.accepted().collect();
    let mut unique = accepted.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        accepted.len(),
        "{}: {accepted:?}",
        table.argument
    );
    for name in table.names() {
        let value = table.parse(name).unwrap();
        assert_eq!(table.name_of(value), name, "{}", table.argument);
    }
    for choice in table.choices {
        for alias in choice.aliases {
            assert_eq!(table.parse(alias), Ok(choice.value), "{alias}");
        }
    }
    if let Some(default) = table.default {
        assert_eq!(table.parse_or_default(None), Ok(default));
        assert!(table.admits(default));
        assert_eq!(table.default_name(), Some(table.name_of(default)));
    }
}

#[test]
fn every_table_names_each_value_once_and_has_its_default() {
    check(&FORMAT);
    check(&AGENT_FORMAT);
    check(&MODEL_FORMAT);
    check(&GROUP_BY);
    check(&MODEL_FIELDS);
    check(&OUTLINE_FORMAT);
    check(&OUTLINE_FIELDS);
    check(&DEPS);
    check(&specforge_ops::registry::CONTRIBUTES);
    // The contribution filter lists the ten declared categories, in the host's read order.
    assert_eq!(
        specforge_ops::registry::CONTRIBUTES
            .names()
            .collect::<Vec<_>>(),
        specforge_protocol_types::DECLARED_CATEGORIES
    );
}

/// ADR 0027 D2: the default lives in the table; the emitter's `#[default]`
/// is its own convenience, and the two agree.
#[test]
fn the_option_defaults_are_the_emitters() {
    let model = ModelOptions::default();
    let outline = OutlineOptions::default();
    assert_eq!(MODEL_FORMAT.default, Some(model.format));
    assert_eq!(GROUP_BY.default, Some(model.group_by));
    assert_eq!(MODEL_FIELDS.default, Some(model.fields));
    assert_eq!(OUTLINE_FORMAT.default, Some(outline.format));
    assert_eq!(OUTLINE_FIELDS.default, Some(outline.detail));
    assert_eq!(DEPS.default, Some(outline.deps));
    // Each value of the model types is listed.
    for value in [GroupBy::Extension, GroupBy::None] {
        assert!(GROUP_BY.admits(value));
    }
    for value in [FieldLevel::None, FieldLevel::Keys, FieldLevel::All] {
        assert!(MODEL_FIELDS.admits(value));
    }
    for value in [OutlineDetail::None, OutlineDetail::Keys, OutlineDetail::All] {
        assert!(OUTLINE_FIELDS.admits(value));
    }
    for value in [
        OutlineFormat::Markdown,
        OutlineFormat::Mermaid,
        OutlineFormat::Dot,
        OutlineFormat::Json,
    ] {
        assert!(OUTLINE_FORMAT.admits(value));
    }
}
