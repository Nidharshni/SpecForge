use specforge_extension_sdk::FieldDescriptor;
use specforge_test::prelude::*;

/// The declared field `name` of `kind`.
fn declared(kind: &str, name: &str) -> FieldDescriptor {
    crate::specforge_extension_build()
        .declaration()
        .entities
        .into_iter()
        .find(|k| k.keyword.as_deref() == Some(kind))
        .unwrap_or_else(|| panic!("{kind} is not declared"))
        .fields
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{kind}.{name} is not declared"))
}

/// A tag is one word from an open set (ADR 0147); `risk` and `direction` are closed sets.
#[specforge_test(
    behavior = "se_register_field_definitions",
    verify = "tag fields are identifiers, and risk and direction are enums of their levels"
)]
fn software_declares_its_tags_and_levels() {
    for (kind, field) in [
        ("behavior", "category"),
        ("behavior", "status"),
        ("behavior", "severity"),
        ("behavior", "diagnostic"),
        ("event", "channel_type"),
        ("event", "category"),
    ] {
        assert_eq!(
            declared(kind, field).field_type,
            "identifier",
            "{kind}.{field}"
        );
    }
    for (kind, field, values) in [
        (
            "invariant",
            "risk",
            &["critical", "high", "medium", "low"][..],
        ),
        (
            "port",
            "direction",
            &["inbound", "outbound", "bidirectional"][..],
        ),
    ] {
        let declared = declared(kind, field);
        assert_eq!(declared.field_type, "enum", "{kind}.{field}");
        assert_eq!(declared.enum_values, values, "{kind}.{field}");
        assert_eq!(declared.default_value, None, "{kind}.{field}");
    }
    for (kind, field) in [("port", "category"), ("event", "channel")] {
        assert_eq!(declared(kind, field).field_type, "string", "{kind}.{field}");
    }
}
