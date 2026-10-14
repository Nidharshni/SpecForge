//! Golden: the SDK's builders must declare what the `software` builtin
//! declares. (The typed round trip of every committed blob's declaration
//! is `crates/specforge-component/tests/declarations.rs`.) This is the pin behind "typed descriptors
//! shared wire-exact with the host" (wayfinder map #1, ticket #2).

use specforge_extension_sdk::{
    ContributionsBuilder, ExtensionDeclaration, ExtensionMeta, HandshakeResponse, PeerDependency,
    SandboxPolicy, prelude::*,
};
use specforge_protocol_types::DeclaredCategory;

fn software_builder() -> ContributionsBuilder {
    let mut meta = ExtensionMeta::new("@specforge/software", "1.0.0");
    meta.description = Some(
        "Software design: behaviors, invariants, events, types and ports, and the checks that \
         keep them consistent"
            .to_string(),
    );
    meta.peer_dependencies = vec![PeerDependency {
        name: "@specforge/product".to_string(),
        version: "^1.0".to_string(),
        optional: true,
    }];
    meta.sandbox_policy = Some(SandboxPolicy {
        max_memory_mb: Some(256),
        max_execution_ms: Some(5000),
    });

    let mut b = ContributionsBuilder::new(meta);
    b.theme_color("#4a90d9");
    b.starter_template(include_str!(
        "../../../extensions/software/src/starter.spec"
    ));
    b.kind("Behavior", |k| {
        k.description("A testable unit of system functionality with a defined contract")
            .testable(true)
            .supports_verify(true)
            .dot_shape("box")
            .dot_color("#1565C0")
            .dot_fillcolor("#E3F2FD")
            .field("contract", |f| {
                f.field_type(FieldType::String)
                    .required()
                    .normative()
                    .headline()
                    .description("The behavioral contract this behavior guarantees");
            })
            .field("invariants", |f| {
                f.field_type(FieldType::ReferenceList)
                    .description("Invariants this behavior enforces")
                    .edge("BehaviorEnforcesInvariant")
                    .target_kind("invariant")
                    .inverse_of("enforced_by");
            })
            .field("types", |f| {
                f.field_type(FieldType::ReferenceList)
                    .description("Type definitions used by this behavior")
                    .edge("BehaviorReferencesType")
                    .target_kind("type");
            })
            .field("ports", |f| {
                f.field_type(FieldType::ReferenceList)
                    .description("Port interfaces this behavior interacts with")
                    .edge("BehaviorUsesPort")
                    .target_kind("port");
            });
    });
    b.rule("W001", |r| {
        r.check(CheckKind::NoOutgoingEdges)
            .target_kind("behavior")
            .edge_type("BehaviorImplementsFeature")
            .severity(ValidationSeverity::Warning)
            .message_template("behavior '{id}' does not implement any feature");
    });
    b
}

/// The pinned wire declaration of the `software` builtin
/// (`crates/specforge-component/tests/declarations/software/`, generated from the
/// vendored blob by `xtask revendor`).
fn pinned(dir: &str, file: &str) -> String {
    let path = specforge_repo_root::repo_root()
        .join("crates/specforge-component/tests/declarations")
        .join(dir)
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A pinned declaration, loaded as a host loads it.
fn load_pinned(dir: &str) -> ExtensionDeclaration {
    let handshake: HandshakeResponse =
        serde_json::from_str(&pinned(dir, "handshake.json")).unwrap();
    ExtensionDeclaration::from_wire(
        handshake,
        |category| {
            Ok(
                serde_json::from_str(&pinned(dir, &format!("describe_{}.json", category.name())))
                    .unwrap(),
            )
        },
        |key| panic!("{dir}: unknown key {key:?}"),
    )
    .unwrap_or_else(|e| panic!("{dir}: {e}"))
}

/// What the SDK builders declare is exactly what the builtin declares for
/// the same contributions.
#[test]
fn the_builders_declare_what_the_builtin_declares() {
    let built = software_builder().declaration();
    let software = load_pinned("software");
    assert_eq!(built.handshake, software.handshake, "handshake");
    let behavior = &built.entities[0];
    let builtin = software
        .entities
        .iter()
        .find(|k| k.name == behavior.name)
        .expect("software declares Behavior");
    assert_eq!(behavior.description, builtin.description);
    assert_eq!(behavior.dot_shape, builtin.dot_shape);
    for field in &behavior.fields {
        let declared = builtin
            .fields
            .iter()
            .find(|f| f.name == field.name)
            .unwrap_or_else(|| panic!("software declares {}", field.name));
        assert_eq!(field, declared, "field {}", field.name);
    }
    assert_eq!(
        built.validation_rules[0], software.validation_rules[0],
        "W001 rule"
    );
}

#[test]
fn contributions_are_read_from_the_declaration() {
    let declaration = software_builder().declaration();
    assert!(declaration.declares(DeclaredCategory::Entities));
    assert!(declaration.declares(DeclaredCategory::ValidationRules));
    assert!(!declaration.handshake.providers);
}

#[test]
fn mock_host_pins_wire_output() {
    use specforge_extension_sdk::testing::MockHost;
    let host = MockHost::new(software_builder());
    host.assert_describe(
        "validation_rules",
        r#"{ "category": "validation_rules", "items": [ { "check": "no_outgoing_edges", "code": "W001", "constraint": null, "edge_type": "BehaviorImplementsFeature", "field": null, "message_template": "behavior '{id}' does not implement any feature", "severity": "warning", "target_kind": "behavior", "wasm_function": null } ] }"#,
    );
}
