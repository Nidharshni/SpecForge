//! `register_entity_enhancements`: an extension adds fields (and verify
//! kinds) to a kind another extension declares; the build registers them
//! after every declaration's own kinds and fields.

use specforge_common::Severity;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::FieldType;
use specforge_registry::entity::EntityRecord;
use specforge_test_macros::test as spec;

use crate::support::{build, check, coded, coded_in, codes, declare, diagnostics, software, span};

/// An extension `name` enhancing `target` with string fields `fields`.
fn enhancer(name: &str, target: &str, fields: &[&str]) -> ExtensionDeclaration {
    declare(name, |c| {
        c.enhance(target, |e| {
            for field in fields {
                e.field(field, |f| {
                    f.field_type(FieldType::String);
                });
            }
        });
    })
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "enhancement fields registered in FieldRegistry"
)]
fn an_enhancement_registers_its_fields_on_the_target_kind() {
    let coverage = declare("@test/coverage", |c| {
        c.enhance("behavior", |e| {
            e.field("coverage_threshold", |f| {
                f.field_type(FieldType::String);
            });
        });
    });
    let build = build([software(), coverage]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    let field = build
        .fields()
        .get("behavior", "coverage_threshold")
        .unwrap();
    assert_eq!(field.kind_name(), "behavior");
    assert_eq!(field.field_type(), FieldType::String);
    assert_eq!(
        field.source_extension(),
        "@test/coverage",
        "an enhancement's fields are its extension's"
    );
    // The kind's own fields stay.
    assert!(build.fields().contains("behavior", "contract"));
    assert!(build.fields().contains("behavior", "invariants"));

    // Two enhancements of one kind, with different fields: both register.
    let build = crate::support::build([
        software(),
        enhancer("@ext/a", "behavior", &["priority"]),
        enhancer("@ext/b", "behavior", &["category"]),
    ]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    assert_eq!(
        build
            .fields()
            .get("behavior", "priority")
            .unwrap()
            .source_extension(),
        "@ext/a"
    );
    assert_eq!(
        build
            .fields()
            .get("behavior", "category")
            .unwrap()
            .source_extension(),
        "@ext/b"
    );
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "unknown target kind produces I004 info diagnostic"
)]
fn an_enhancement_of_an_unknown_kind_is_i004() {
    let build = build([
        software(),
        enhancer("@test/ext", "nonexistent_kind", &["extra"]),
    ]);

    let i004 = coded(&build, "I004");
    assert_eq!(i004.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(diagnostics(&build).len(), 1);
    assert_eq!(i004[0].severity, Severity::Info);
    assert_eq!(
        i004[0].message,
        "extension '@test/ext': entity enhancement targets unknown kind 'nonexistent_kind' (extension may not be installed)"
    );
    assert_eq!(codes(&build), ["I004"]);
    assert!(!build.fields().contains("nonexistent_kind", "extra"));
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "an enhancement of an unknown kind whose extension's optional peer is not loaded is skipped silently"
)]
fn an_enhancement_of_a_kind_whose_optional_peer_is_not_loaded_is_skipped_silently() {
    let enhancing = || {
        declare("@test/ext", |c| {
            c.optional_peer("@specforge/product", "^1");
            c.enhance("module", |e| {
                e.field("layer", |f| {
                    f.field_type(FieldType::String);
                });
            });
        })
    };

    // The optional peer is not loaded: the project doesn't use it, nothing
    // to say.
    let build = build([software(), enhancing()]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    assert!(!build.fields().contains("module", "layer"));

    // The peer is loaded, yet declares no such kind: a real mismatch, I004.
    let product = declare("@specforge/product", |c| {
        c.kind("Feature", |k| {
            k.keyword("feature");
        });
    });
    let build = crate::support::build([software(), product, enhancing()]);
    let i004 = coded(&build, "I004");
    assert_eq!(i004.len(), 1, "{:?}", diagnostics(&build));
    assert!(i004[0].message.contains("'module'"), "{}", i004[0].message);
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "an enhancement with verify kinds makes its target kind testable"
)]
fn an_enhancement_with_verify_kinds_makes_its_target_testable() {
    let untestable = declare("@specforge/software", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior").testable(false).supports_verify(false);
        });
    });
    let testing = declare("@specforge/testing", |c| {
        c.enhance("behavior", |e| {
            e.verify_kinds(&["unit", "contract"]);
        });
    });

    let before = build([untestable.clone()]);
    assert!(!before.kinds().get("behavior").unwrap().testable);

    let build = build([untestable, testing]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    let behavior = build.kinds().get("behavior").unwrap();
    assert!(behavior.testable && behavior.supports_verify);
    assert_eq!(behavior.allowed_verify_kinds, ["unit", "contract"]);
    assert_eq!(
        behavior.source_extension, "@specforge/software",
        "the kind stays its declarer's"
    );
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "enhancement field does NOT overwrite existing kind-level field"
)]
fn an_enhancement_field_never_overwrites_a_kind_field() {
    // `contract` is a block field of software's `behavior`; the enhancement
    // declares it again as a string.
    let build = build([software(), enhancer("@test/ext", "behavior", &["contract"])]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    let contract = build.fields().get("behavior", "contract").unwrap();
    assert_eq!(contract.field_type(), FieldType::Block);
    assert_eq!(contract.source_extension(), "@specforge/software");
}

/// Two extensions each adding `owner` to `behavior`, with different types,
/// and a `<type>_note` field each.
fn owner_enhancer(name: &str, field_type: FieldType) -> ExtensionDeclaration {
    declare(name, |c| {
        c.enhance("behavior", |e| {
            e.field("owner", |f| {
                f.field_type(field_type);
            });
            e.field(&format!("{}_note", field_type.as_str()), |f| {
                f.field_type(FieldType::String);
            });
        });
    })
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "registration order follows extensions array"
)]
fn enhancements_register_in_load_order() {
    let a = owner_enhancer("@test/a", FieldType::String);
    let b = owner_enhancer("@test/b", FieldType::Reference);

    let a_first = build([software(), a.clone(), b.clone()]);
    let owner = a_first.fields().get("behavior", "owner").unwrap();
    assert_eq!(owner.source_extension(), "@test/a");
    assert_eq!(owner.field_type(), FieldType::String);

    // The load order decides which `owner` wins.
    let b_first = build([software(), b.clone(), a.clone()]);
    let owner = b_first.fields().get("behavior", "owner").unwrap();
    assert_eq!(owner.source_extension(), "@test/b");
    assert_eq!(owner.field_type(), FieldType::Reference);

    // The same order always gives the same result.
    for _ in 0..3 {
        let again = build([software(), a.clone(), b.clone()]);
        assert_eq!(
            again
                .fields()
                .get("behavior", "owner")
                .unwrap()
                .source_extension(),
            "@test/a"
        );
    }
}

#[spec(
    behavior = "register_entity_enhancements",
    verify = "Register Entity Enhancements: entity enhancement registration holds — manifests_validated, enhancement_registered_emitted, registration_before_resolve, registration_order_deterministic"
)]
fn enhancement_registration_holds() {
    let a = owner_enhancer("@test/a", FieldType::String);
    let b = owner_enhancer("@test/b", FieldType::Reference);
    let build = build([software(), a, b]);

    // manifests_validated: every declaration passes its own checks.
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    // enhancement_registered_emitted: each registered field records the
    // extension, target kind, field and type the event carries.
    let owner = build.fields().get("behavior", "owner").unwrap();
    assert_eq!(owner.kind_name(), "behavior");
    assert_eq!(owner.source_extension(), "@test/a");
    assert_eq!(owner.field_type(), FieldType::String);
    let note = build.fields().get("behavior", "reference_note").unwrap();
    assert_eq!(note.source_extension(), "@test/b");

    // registration_before_resolve: the registries the build hands on
    // already accept an enhanced field on a parsed entity (no W020).
    let diagnostics = check(
        &build,
        &[
            EntityRecord::new("behavior", "b1", span("main.spec")).with_fields(&[
                "owner",
                "string_note",
                "reference_note",
            ]),
        ],
    );
    let unknown = coded_in(&diagnostics, "W020");
    assert!(unknown.is_empty(), "{unknown:?}");
    // A field nobody declares is still W020.
    let diagnostics = check(
        &build,
        &[EntityRecord::new("behavior", "b2", span("main.spec")).with_fields(&["nobody"])],
    );
    let unknown = coded_in(&diagnostics, "W020");
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert_eq!(unknown[0].code, "W020");

    // registration_order_deterministic: the load order decides which
    // `owner` wins.
    let swapped = crate::support::build([
        software(),
        owner_enhancer("@test/b", FieldType::Reference),
        owner_enhancer("@test/a", FieldType::String),
    ]);
    assert_eq!(
        swapped
            .fields()
            .get("behavior", "owner")
            .unwrap()
            .source_extension(),
        "@test/b"
    );
}
