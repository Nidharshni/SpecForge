//! `registry_build_kinds`: every declared kind is in the build's
//! `KindRegistry` under its keyword, with what its declaration says and
//! nothing the host assumes.

use specforge_common::{DiagnosticData, Severity};
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_test_macros::test as spec;

use crate::support::{build, coded, codes, declare, diagnostics, product, software};

/// `@other/ext`, declaring `behavior` again.
fn other_behavior() -> ExtensionDeclaration {
    declare("@other/ext", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
        });
    })
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "every declared kind is registered under its keyword, naming the extension that declared it"
)]
fn every_declared_kind_is_registered_under_its_keyword() {
    let unkeyworded = declare("@test/plain", |c| {
        c.kind("widget", |k| {
            k.description("A kind declared without a keyword");
        });
    });
    let build = build([software(), product(), unkeyworded]);

    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    let mut keywords: Vec<&str> = build.kinds.keywords().map(String::as_str).collect();
    keywords.sort();
    assert_eq!(keywords, ["behavior", "feature", "invariant", "widget"]);
    assert_eq!(build.kinds.len(), 4);
    for (keyword, extension) in [
        ("behavior", "@specforge/software"),
        ("invariant", "@specforge/software"),
        ("feature", "@specforge/product"),
        ("widget", "@test/plain"),
    ] {
        let entry = build.kinds.get(keyword).expect(keyword);
        assert_eq!(entry.kind_name, keyword);
        assert_eq!(entry.source_extension, extension, "{keyword}");
    }
    // The entry embeds the declared descriptor: its declared name stays.
    assert_eq!(
        build.kinds.get("behavior").unwrap().declared.name,
        "Behavior"
    );
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a kind's declared flags and presentation reach its registry entry"
)]
fn a_kinds_declared_flags_and_presentation_reach_its_entry() {
    let flagged = declare("@test/ext", |c| {
        c.kind("Project", |k| {
            k.keyword("project")
                .singleton(true)
                .description("A testable unit of system functionality");
        });
        c.kind("Task", |k| {
            k.keyword("task");
        });
    });
    let build = build([software(), flagged]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    let behavior = build.kinds.get("behavior").unwrap();
    assert!(behavior.testable);
    assert!(behavior.supports_verify);
    assert!(!behavior.declared.singleton);
    assert_eq!(
        behavior.declared.semantic_token.as_deref(),
        Some("function")
    );
    assert_eq!(behavior.declared.lsp_icon.as_deref(), Some("Method"));
    assert_eq!(behavior.declared.dot_shape.as_deref(), Some("ellipse"));
    assert_eq!(behavior.declared.description, None);
    assert!(build.kinds.get("invariant").unwrap().testable);

    let project = build.kinds.get("project").unwrap();
    assert!(project.declared.singleton);
    assert_eq!(
        project.declared.description.as_deref(),
        Some("A testable unit of system functionality")
    );
    let task = build.kinds.get("task").unwrap();
    assert!(!task.declared.singleton, "singleton defaults to false");
    assert!(!task.supports_verify);
    assert_eq!(task.declared.semantic_token, None);
    assert_eq!(task.declared.lsp_icon, None);
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a kind is testable only when its declaration says so"
)]
fn a_kind_is_testable_only_when_declared_so() {
    let undeclared = declare("@test/ext", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
        });
    });
    let build = build([software(), product(), undeclared]);

    let mut testable: Vec<&str> = build
        .kinds
        .iter()
        .filter(|(_, entry)| entry.testable)
        .map(|(keyword, _)| keyword.as_str())
        .collect();
    testable.sort();
    assert_eq!(testable, ["behavior", "invariant"]);
    assert!(!build.kinds.get("feature").unwrap().testable);
    let thing = build.kinds.get("thing").unwrap();
    assert!(!thing.testable, "the host assumes no testability");
    assert!(thing.allowed_verify_kinds.is_empty());
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a kind's verify kinds are exactly the ones it declares"
)]
fn a_kinds_verify_kinds_are_exactly_the_declared_ones() {
    let declared = declare("@test/ext", |c| {
        c.kind("Scenario", |k| {
            k.keyword("scenario")
                .testable(true)
                .supports_verify(true)
                .verify_kinds(&["unit", "chaos"]);
        });
        c.kind("Note", |k| {
            k.keyword("note").testable(true).supports_verify(true);
        });
    });
    let build = build([declared]);

    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    assert_eq!(
        build.kinds.get("scenario").unwrap().allowed_verify_kinds,
        ["unit", "chaos"]
    );
    assert!(
        build
            .kinds
            .get("note")
            .unwrap()
            .allowed_verify_kinds
            .is_empty(),
        "no verify kind is allowed unless declared"
    );
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a kind's lifecycle_field must name a field it declares"
)]
fn a_lifecycle_field_must_name_a_field_the_kind_declares() {
    let declared = declare("@test/ext", |c| {
        c.shared_field("phase", |f| {
            f.field_type(FieldType::String);
        });
        c.kind("Task", |k| {
            k.keyword("task").lifecycle_field("stage");
            k.field("stage", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.kind("Epic", |k| {
            k.keyword("epic").lifecycle_field("phase");
        });
        c.kind("Note", |k| {
            k.keyword("note").lifecycle_field("status");
        });
        c.kind("Idea", |k| {
            k.keyword("idea");
        });
    });
    let build = build([declared]);

    let lifecycle = |kind: &str| build.kinds.get(kind).unwrap().lifecycle_field.clone();
    assert_eq!(lifecycle("task").as_deref(), Some("stage"));
    // An extension-level shared field is one of the kind's fields.
    assert_eq!(lifecycle("epic").as_deref(), Some("phase"));
    assert_eq!(lifecycle("idea"), None);
    // `note` declares no `status`: refused, and the kind still registers.
    assert_eq!(lifecycle("note"), None);
    let refused = coded(&build, "W021");
    assert_eq!(refused.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(refused[0].severity, Severity::Warning);
    assert!(
        refused[0].message.contains("lifecycle_field 'status'"),
        "{}",
        refused[0].message
    );
    assert_eq!(codes(&build), ["W021"]);
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a keyword a later extension declares again is E026 and the first in load order keeps it"
)]
fn a_keyword_declared_twice_is_e026_and_the_first_keeps_it() {
    let alone = build([software()]);
    assert!(coded(&alone, "E026").is_empty());

    let build = build([software(), other_behavior()]);
    let e026 = coded(&build, "E026");
    assert_eq!(e026.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(e026[0].severity, Severity::Error);
    assert_eq!(
        e026[0].message,
        "entity kind 'behavior' registered by '@other/ext' conflicts with '@specforge/software' (first registration wins)"
    );
    assert!(matches!(
        e026[0].data.as_deref(),
        Some(DiagnosticData::ShadowedKeyword { keyword }) if keyword == "behavior"
    ));
    let behavior = build.kinds.get("behavior").unwrap();
    assert_eq!(behavior.source_extension, "@specforge/software");
    assert!(behavior.testable, "the first declaration's flags stay");

    // Load order decides: the other way round, the other one keeps it.
    let swapped = crate::support::build([other_behavior(), software()]);
    assert_eq!(
        swapped.kinds.get("behavior").unwrap().source_extension,
        "@other/ext"
    );
    assert_eq!(coded(&swapped, "E026").len(), 1);
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a testable kind without verify support is W017"
)]
fn a_testable_kind_without_verify_support_is_w017() {
    let build = build([declare("@test/ext", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing").testable(true).supports_verify(false);
        });
        c.kind("Done", |k| {
            k.keyword("done").testable(true).supports_verify(true);
        });
        c.kind("Plain", |k| {
            k.keyword("plain").testable(false).supports_verify(false);
        });
    })]);
    let w017 = coded(&build, "W017");
    assert_eq!(w017.len(), 1, "{w017:?}");
    assert_eq!(w017[0].severity, Severity::Warning);
    assert!(w017[0].message.contains("'thing'"), "{}", w017[0].message);
    assert!(
        w017[0].message.contains("'@test/ext'"),
        "{}",
        w017[0].message
    );
    assert_eq!(codes(&build), ["W017"]);
    assert!(
        build.kinds.contains("thing"),
        "W017 is advisory: the kind registers"
    );

    // Consistent flags (software's testable kinds support verify): nothing.
    assert!(coded(&crate::support::build([software()]), "W017").is_empty());
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "a kind that accepts verify statements but is not testable produces no diagnostic"
)]
fn a_kind_supporting_verify_without_being_testable_is_not_reported() {
    let build = build([declare("@test/ext", |c| {
        c.kind("Property", |k| {
            k.keyword("property").testable(false).supports_verify(true);
        });
    })]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    let property = build.kinds.get("property").unwrap();
    assert!(!property.testable && property.supports_verify);
}

#[spec(
    behavior = "registry_build_kinds",
    verify = "Registry Build Registers Kinds: kind registration holds — declarations_in_load_order, kinds_registered, first_wins, flags_consistent"
)]
fn kind_registration_holds() {
    // declarations_in_load_order: software, then product (its peer), then
    // an extension declaring `behavior` again and a testable kind without
    // verify support.
    let late = declare("@test/late", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
        });
        c.kind("Probe", |k| {
            k.keyword("probe").testable(true);
        });
    });
    let build = build([software(), product(), late]);

    // kinds_registered: every kind, with the extension that declared it.
    for (keyword, extension) in [
        ("behavior", "@specforge/software"),
        ("invariant", "@specforge/software"),
        ("feature", "@specforge/product"),
        ("probe", "@test/late"),
    ] {
        assert_eq!(
            build.kinds.get(keyword).expect(keyword).source_extension,
            extension
        );
    }
    assert_eq!(build.kinds.len(), 4);

    // first_wins: the repeated keyword is E026, and the first keeps it.
    let e026 = coded(&build, "E026");
    assert_eq!(e026.len(), 1);
    assert!(e026[0].message.contains("'@test/late'"));
    assert!(build.kinds.get("behavior").unwrap().testable);

    // flags_consistent: W017 names the testable kind without verify support.
    let w017 = coded(&build, "W017");
    assert_eq!(w017.len(), 1);
    assert!(w017[0].message.contains("'probe'"));

    // No error but the collision.
    let errors: Vec<&str> = diagnostics(&build)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(errors, ["E026"]);
}
