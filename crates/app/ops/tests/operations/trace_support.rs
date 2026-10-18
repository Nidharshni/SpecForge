//! Registries for trace tests: what three made-up extensions declare.

use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_registry::{RegistryBuild, build_registries};

/// `@t/soft` declares behavior and invariant, `@t/prod` declares feature,
/// `@t/formal` adds a field to behavior, as a registry build. Of behavior's
/// reference fields only `invariants` and `features` are expected edges:
/// `ports` points at a kind nothing loads, `depends_on` at behavior itself,
/// `satisfies` comes from another extension, and `contract` is a string, not
/// a reference. A feature has no reference field: a relation is written on
/// its owning field only (ADR 0150). `invariants` and `features` name the
/// word the target kind reads the relation by in `inverse_of`, neither a
/// field of its target kind.
pub fn build() -> RegistryBuild {
    build_with(false)
}

/// [`build`], and `@t/formal` also adds a required reference `parent`
/// (behavior to behavior) to behavior: a required reference is expected
/// wherever it comes from.
pub fn build_with_required_parent() -> RegistryBuild {
    build_with(true)
}

fn build_with(required_parent: bool) -> RegistryBuild {
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
    });
    let mut formal = ContributionsBuilder::new(ExtensionMeta::new("@t/formal", "1.0.0"));
    formal.meta.peer_dependencies = vec![peer("@t/soft")];
    formal.enhance("behavior", |e| {
        e.field("satisfies", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("behavior_satisfies")
                .target_kind("invariant");
        });
        if required_parent {
            e.field("parent", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("behavior_parent")
                    .target_kind("behavior")
                    .required();
            });
        }
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
    declare_edges(&mut formal, &["behavior_satisfies", "behavior_parent"]);
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
