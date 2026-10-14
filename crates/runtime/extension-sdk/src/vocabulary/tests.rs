use super::*;

const STATUS: Vocabulary = Vocabulary::new(&["draft", "live", "retired"]).absent_as("draft");
const LIVE: &str = STATUS.member("live");

#[test]
fn a_vocabulary_keeps_its_order_and_absent_value() {
    assert_eq!(STATUS.values(), ["draft", "live", "retired"]);
    assert_eq!(STATUS.absent(), Some("draft"));
    assert_eq!(Vocabulary::new(&["a"]).absent(), None);
    assert!(STATUS.contains("retired"));
    assert!(!STATUS.contains("gone"));
}

#[test]
fn read_is_the_written_value_else_the_absent_one() {
    assert_eq!(STATUS.read(Some("live")), Some("live"));
    assert_eq!(STATUS.read(None), Some("draft"));
    assert_eq!(STATUS.read(Some("")), Some("draft"));
    assert_eq!(Vocabulary::new(&["a"]).read(None), None);
}

#[test]
fn position_ranks_and_refuses_strangers() {
    assert_eq!(STATUS.position("draft"), Some(0));
    assert_eq!(STATUS.position("retired"), Some(2));
    assert_eq!(STATUS.position("gone"), None);
}

#[test]
fn member_and_index_name_a_value() {
    assert_eq!(LIVE, "live");
    assert_eq!(STATUS.index("retired"), 2);
    const RETIRED_AT: usize = STATUS.index("retired");
    assert_eq!(RETIRED_AT, 2);
}

#[test]
#[should_panic]
fn an_empty_vocabulary_panics() {
    Vocabulary::new(&[]);
}

#[test]
#[should_panic]
fn a_repeated_value_panics() {
    Vocabulary::new(&["a", "a"]);
}

#[test]
#[should_panic]
fn an_empty_value_panics() {
    Vocabulary::new(&["a", ""]);
}

#[test]
#[should_panic]
fn an_absent_value_outside_the_vocabulary_panics() {
    Vocabulary::new(&["a"]).absent_as("b");
}

#[test]
#[should_panic]
fn a_member_outside_the_vocabulary_panics() {
    STATUS.member(std::hint::black_box("gone"));
}

#[test]
#[should_panic]
fn an_index_outside_the_vocabulary_panics() {
    STATUS.index(std::hint::black_box("gone"));
}

fn declared_field(
    f: impl FnOnce(&mut crate::FieldBuilder),
) -> specforge_protocol_types::FieldDescriptor {
    let mut c = crate::ContributionsBuilder::new(crate::ExtensionMeta::new("@acme/v", "0.1.0"));
    c.kind("thing", |k| {
        k.field("state", f);
    });
    c.declaration().entities[0].fields[0].clone()
}

#[test]
fn a_vocabulary_declares_an_enum_with_its_values_and_default() {
    let field = declared_field(|f| {
        f.vocabulary(&STATUS);
    });
    assert_eq!(field.field_type, "enum");
    assert_eq!(field.enum_values, ["draft", "live", "retired"]);
    assert_eq!(field.default_value.as_deref(), Some("draft"));

    let field = declared_field(|f| {
        f.vocabulary(&Vocabulary::new(&["a", "b"]));
    });
    assert_eq!(field.default_value, None);
}

#[test]
#[should_panic(expected = "FieldBuilder::vocabulary")]
fn an_enum_type_without_a_vocabulary_panics() {
    declared_field(|f| {
        f.field_type(crate::FieldType::Enum);
    });
}
