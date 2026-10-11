//! One extension vocabulary: what an SDK-built extension declares is what
//! the host's registry build reads. Every field type the SDK can name
//! registers (no W019), and the rules `specforge new --extension` and the
//! greet fixture write parse (no W112) and fire.

use specforge_common::{SourceSpan, Sym};
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta, prelude::*};
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::build_registries;
use specforge_registry::entity::{EntityRecord, RuleInput};
use specforge_registry::rules::NoVerdicts;
use specforge_wasm::protocol::load_declaration;
use specforge_wasm::testing::InProcessRuntime;

fn extension() -> ContributionsBuilder {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new("@you/vocab", "0.1.0"));
    c.kind("thing", |k| {
        for t in FieldType::ALL {
            k.field(&format!("f_{t}"), |f| {
                f.field_type(*t);
                if *t == FieldType::Enum {
                    f.enum_values(&["warm", "formal"]);
                }
            });
        }
        k.field("description", |f| {
            f.field_type(FieldType::String);
        });
        k.field("style", |f| {
            f.field_type(FieldType::String);
        });
    });
    // The rule `specforge new --extension` scaffolds.
    c.rule("W900", |r| {
        r.check(CheckKind::MissingRequiredField);
        r.target_kind("thing");
        r.field("description");
        r.severity(ValidationSeverity::Warning);
        r.message_template("thing '{id}' is missing a description");
    });
    // The greet fixture's rule.
    c.rule("E901", |r| {
        r.check(CheckKind::FieldValueConstraint);
        r.target_kind("thing");
        r.field("style");
        r.constraint(|fc| {
            fc.kind(ConstraintKind::Matches);
            fc.pattern("^(warm|formal)$");
        });
        r.severity(ValidationSeverity::Error);
        r.message_template("thing '{id}' has unknown style");
    });
    c
}

/// The declaration `build` declares, as the host loads it: through the
/// one loader, from the extension served in process.
fn loaded(
    build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static,
) -> ExtensionDeclaration {
    let name = build().meta.name.clone();
    let runtime = InProcessRuntime::new().with(build);
    let loaded = load_declaration(&runtime, &name).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    loaded.declaration
}

fn entity(id: &str, fields: &[(&str, &str)]) -> EntityRecord {
    let span = SourceSpan {
        file: Sym::new("test.spec"),
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 0,
    };
    fields.iter().fold(
        EntityRecord::new("thing", id, &span),
        |record, (key, text)| record.with_field(key, text),
    )
}

/// The rules' input over `entities`, with no edges and no spec root.
fn rules_over(entities: &[EntityRecord]) -> RuleInput<'_> {
    RuleInput {
        entities,
        edges: &[],
        spec_root: std::path::Path::new(""),
    }
}

#[test]
fn sdk_vocabulary_round_trips_through_the_registry_build() {
    let build = build_registries(vec![loaded(extension)]);

    let unread: Vec<_> = build
        .diagnostics()
        .filter(|d| d.code == "W019" || d.code == "W112")
        .collect();
    assert!(unread.is_empty(), "host could not read: {unread:?}");
    for t in FieldType::ALL {
        assert!(
            build.fields().contains("thing", &format!("f_{t}")),
            "field type {t} did not register"
        );
    }

    for code in ["W900", "E901"] {
        assert!(
            build.rules().iter().any(|rule| rule.code() == code),
            "rule {code} not registered"
        );
    }
    // What the rule `code` reports over `entities`.
    let fired = |code: &str, entities: &[specforge_registry::entity::EntityRecord]| {
        build
            .rules()
            .check(&rules_over(entities), &NoVerdicts)
            .into_iter()
            .filter(|d| d.code == code)
            .collect::<Vec<_>>()
    };
    let bare = [entity("bare", &[("style", "loud")])];
    let fine = [entity(
        "fine",
        &[("description", "a thing"), ("style", "warm")],
    )];

    let w900 = fired("W900", &bare);
    assert_eq!(w900.len(), 1, "{w900:?}");
    assert_eq!(w900[0].message, "thing 'bare' is missing a description");
    let g101 = fired("E901", &bare);
    assert_eq!(g101.len(), 1, "{g101:?}");
    assert_eq!(g101[0].code, "E901");
    assert!(fired("W900", &fine).is_empty());
    assert!(fired("E901", &fine).is_empty());
}

/// Rules written by SDK releases before the vocabulary was shared still
/// load: the host reads their check names as aliases.
#[test]
fn older_sdk_check_names_still_load() {
    let mut ext = loaded(extension);
    for (rule, old) in ext
        .validation_rules
        .iter_mut()
        .zip(["missing_field", "field_constraint"])
    {
        rule.check = old.to_string();
    }
    ext.entities[0].fields[0].field_type = "boolean".to_string();

    let build = build_registries(vec![ext]);
    let unread: Vec<_> = build
        .diagnostics()
        .filter(|d| d.code == "W019" || d.code == "W112")
        .collect();
    assert!(unread.is_empty(), "host could not read: {unread:?}");
    assert!(build.rules().iter().any(|rule| rule.code() == "W900"));
    assert!(build.rules().iter().any(|rule| rule.code() == "E901"));
}

/// Every constraint kind the SDK can name loads on the check that reads it:
/// `non_empty`, `one_of` and `matches` on `field_value_constraint`,
/// `when_field_equals` on `conditional_field_required`, `one_of` on
/// `verify_kind_allowlist`.
#[test]
fn sdk_constraint_kinds_load_for_the_checks_that_read_them() {
    let uses = [
        (ConstraintKind::NonEmpty, CheckKind::FieldValueConstraint),
        (ConstraintKind::OneOf, CheckKind::FieldValueConstraint),
        (ConstraintKind::Matches, CheckKind::FieldValueConstraint),
        (
            ConstraintKind::WhenFieldEquals,
            CheckKind::ConditionalFieldRequired,
        ),
        (ConstraintKind::OneOf, CheckKind::VerifyKindAllowlist),
    ];
    for kind in ConstraintKind::ALL {
        assert!(
            uses.iter().any(|(k, _)| k == kind),
            "constraint kind {kind} has no check that reads it"
        );
    }
    let build = move || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@you/constraints", "0.1.0"));
        // `thing` accepts verify statements: the allowlist reads them.
        c.kind("thing", |k| {
            k.supports_verify(true);
            k.field("status", |f| {
                f.field_type(FieldType::String);
            });
            k.field("reason", |f| {
                f.field_type(FieldType::String);
            });
        });
        for (i, (kind, check)) in uses.iter().enumerate() {
            c.rule(&format!("X{i:03}"), |r| {
                r.check(*check);
                r.target_kind("thing");
                if *check != CheckKind::VerifyKindAllowlist {
                    r.field("reason");
                }
                r.constraint(|fc| {
                    fc.kind(*kind);
                    match kind {
                        ConstraintKind::Matches => {
                            fc.pattern("^[a-z]+$");
                        }
                        ConstraintKind::WhenFieldEquals => {
                            fc.pattern("status").values(&["deferred"]);
                        }
                        _ => {
                            fc.values(&["unit", "a"]);
                        }
                    }
                });
                r.message_template("thing '{id}'");
            });
        }
        c
    };

    let build = build_registries(vec![loaded(build)]);
    let unread: Vec<_> = build
        .diagnostics()
        .filter(|d| d.code == "W019" || d.code == "W112")
        .collect();
    assert!(unread.is_empty(), "host could not read: {unread:?}");
    for (i, (kind, _)) in uses.iter().enumerate() {
        let code = format!("X{i:03}");
        let rule = build
            .rules()
            .iter()
            .find(|rule| rule.code() == code)
            .unwrap_or_else(|| panic!("rule {code} not registered"));
        assert_eq!(
            rule.describe()["constraint"]["kind"],
            kind.as_str(),
            "rule {code}"
        );
    }
}
