//! Registries for trace tests: what two made-up extensions declare.

use specforge_extension_sdk::prelude::*;
use specforge_registry::{
    FieldRegistry, FieldRegistryEntry, KindRegistry, KindRegistryEntry, RegistryBuild,
    build_registries,
};

fn kind(name: &str, extension: &str) -> KindRegistryEntry {
    KindRegistryEntry {
        kind_name: name.to_string(),
        source_extension: extension.to_string(),
        testable: false,
        supports_verify: false,
        allowed_verify_kinds: Vec::new(),
        lifecycle_field: None,
        ..Default::default()
    }
}

/// A reference-list field `kind.name -> target`, declared by `extension`.
pub fn reference(
    kind: &str,
    name: &str,
    target: &str,
    extension: &str,
    inverse_of: Option<&str>,
) -> FieldRegistryEntry {
    FieldRegistryEntry::new(
        kind,
        extension,
        specforge_registry::FieldDescriptor {
            name: name.to_string(),
            field_type: "reference_list".to_string(),
            edge: Some(format!("{kind}_{name}")),
            target_kind: Some(target.to_string()),
            inverse_of: inverse_of.map(str::to_string),
            ..Default::default()
        },
    )
    .unwrap()
}

/// `@t/soft` declares behavior and invariant, `@t/prod` declares feature,
/// `@t/formal` adds a field to behavior. Of behavior's reference fields
/// only `invariants` and `features` are expected edges: `ports` points at a
/// kind nothing loads, `depends_on` at behavior itself, `satisfies` comes
/// from another extension, and `contract` is a string, not a reference.
/// `feature.behaviors` is expected of features.
pub fn registries() -> (FieldRegistry, KindRegistry) {
    let mut kinds = KindRegistry::new();
    kinds.register(kind("behavior", "@t/soft"));
    kinds.register(kind("invariant", "@t/soft"));
    kinds.register(kind("feature", "@t/prod"));

    let mut fields = FieldRegistry::new();
    fields.register(reference(
        "behavior",
        "invariants",
        "invariant",
        "@t/soft",
        Some("enforced_by"),
    ));
    fields.register(reference(
        "behavior",
        "features",
        "feature",
        "@t/soft",
        Some("behaviors"),
    ));
    fields.register(reference("behavior", "ports", "port", "@t/soft", None));
    fields.register(reference(
        "behavior",
        "depends_on",
        "behavior",
        "@t/soft",
        None,
    ));
    fields.register(reference(
        "behavior",
        "satisfies",
        "invariant",
        "@t/formal",
        None,
    ));
    // `contract` is a reference-shaped field of the string type.
    let mut declared = reference("behavior", "contract", "invariant", "@t/soft", None)
        .declared()
        .clone();
    declared.field_type = "string".to_string();
    fields.register(FieldRegistryEntry::new("behavior", "@t/soft", declared).unwrap());
    fields.register(reference(
        "feature",
        "behaviors",
        "behavior",
        "@t/prod",
        None,
    ));
    (fields, kinds)
}

/// [`registries`] as a registry build, what a project view holds: the same
/// extensions, declared with the SDK and put through `build_registries`.
pub fn build() -> RegistryBuild {
    /// A reference-list field `name -> target` on the kind being declared.
    fn reference_field(
        k: &mut KindBuilder,
        kind: &str,
        name: &str,
        target: &str,
        inverse_of: Option<&str>,
    ) {
        k.field(name, |f| {
            f.field_type(FieldType::ReferenceList)
                .edge(&format!("{kind}_{name}"))
                .target_kind(target);
            if let Some(inverse_of) = inverse_of {
                f.inverse_of(inverse_of);
            }
        });
    }
    let mut soft = ContributionsBuilder::new(ExtensionMeta::new("@t/soft", "1.0.0"));
    soft.kind("behavior", |k| {
        k.keyword("behavior");
        reference_field(
            k,
            "behavior",
            "invariants",
            "invariant",
            Some("enforced_by"),
        );
        reference_field(k, "behavior", "features", "feature", Some("behaviors"));
        reference_field(k, "behavior", "ports", "port", None);
        reference_field(k, "behavior", "depends_on", "behavior", None);
        // `contract` is a reference-shaped field of the string type.
        k.field("contract", |f| {
            f.field_type(FieldType::String)
                .edge("behavior_contract")
                .target_kind("invariant");
        });
    });
    soft.kind("invariant", |k| {
        k.keyword("invariant");
    });
    let mut prod = ContributionsBuilder::new(ExtensionMeta::new("@t/prod", "1.0.0"));
    prod.meta.peer_dependencies = vec![peer("@t/soft")];
    prod.kind("feature", |k| {
        k.keyword("feature");
        reference_field(k, "feature", "behaviors", "behavior", None);
    });
    let mut formal = ContributionsBuilder::new(ExtensionMeta::new("@t/formal", "1.0.0"));
    formal.meta.peer_dependencies = vec![peer("@t/soft")];
    formal.enhance("behavior", "@t/formal", |e| {
        e.field("satisfies", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("behavior_satisfies")
                .target_kind("invariant");
        });
    });
    // Every edge label a field names is declared by that field's extension.
    fn declare_edges(extension: &mut ContributionsBuilder, labels: &[&str]) {
        for label in labels {
            extension.edge(label, |e| {
                e.description("A reference field's edge");
            });
        }
    }
    declare_edges(
        &mut soft,
        &[
            "behavior_invariants",
            "behavior_features",
            "behavior_ports",
            "behavior_depends_on",
            "behavior_contract",
        ],
    );
    declare_edges(&mut prod, &["feature_behaviors"]);
    declare_edges(&mut formal, &["behavior_satisfies"]);
    let build = build_registries(vec![
        soft.declaration(),
        prod.declaration(),
        formal.declaration(),
    ]);
    // `ports` points at a kind nothing loads, and `features` at a kind of an
    // extension that is not a peer of `@t/soft`: both W021, as the fixture
    // means them.
    let codes: Vec<&str> = build.diagnostics().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["W021", "W021"]);
    build
}

fn peer(name: &str) -> PeerDependency {
    PeerDependency {
        name: name.to_string(),
        version: "^1".to_string(),
        optional: false,
    }
}
