//! `registry_build_fields`: every declared field is in the build's
//! `FieldRegistry` for its kind, embedding its descriptor, typed with one of
//! the protocol's field types.

use specforge_common::Severity;
use specforge_extension_sdk::Vocabulary;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::{
    FieldDescriptor, FieldRegistryEntry, FieldType, ProofRole, UnknownFieldType,
};
use specforge_test_macros::test as spec;

use crate::support::{build, coded, codes, declare, diagnostics, software};

#[spec(
    behavior = "registry_build_fields",
    verify = "every kind's declared fields and its extension's shared fields are registered for that kind"
)]
fn every_kinds_fields_and_shared_fields_are_registered() {
    let shared = declare("@test/ext", |c| {
        c.shared_field("owner", |f| {
            f.field_type(FieldType::String);
        });
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("estimate", |f| {
                f.field_type(FieldType::Integer);
            });
        });
        c.kind("Epic", |k| {
            k.keyword("epic");
        });
    });
    let build = build([software(), shared]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    assert!(build.fields().contains("behavior", "contract"));
    assert!(build.fields().contains("behavior", "invariants"));
    assert_eq!(
        build
            .fields()
            .get("behavior", "contract")
            .unwrap()
            .source_extension(),
        "@specforge/software"
    );
    // `invariant` declares no field.
    assert!(build.fields().fields_for_kind("invariant").is_empty());
    // A shared field reaches every kind of its extension, and only those.
    for kind in ["task", "epic"] {
        let owner = build.fields().get(kind, "owner").expect(kind);
        assert_eq!(owner.kind_name(), kind);
        assert_eq!(owner.source_extension(), "@test/ext");
    }
    assert!(!build.fields().contains("behavior", "owner"));
    assert!(build.fields().contains("task", "estimate"));
    assert!(!build.fields().contains("epic", "estimate"));
}

#[spec(
    behavior = "registry_build_fields",
    verify = "each field type and its _type alias register as that type"
)]
fn each_field_type_and_its_alias_registers_as_that_type() {
    let mut declaration = declare("@test/ext", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
            for field_type in FieldType::ALL {
                for name in [
                    field_type.as_str().to_string(),
                    format!("{}_alias", field_type.as_str()),
                ] {
                    k.field(&name, |f| {
                        if *field_type == FieldType::Enum {
                            f.vocabulary(&Vocabulary::new(&["draft", "done"]));
                        } else {
                            f.field_type(*field_type);
                        }
                    });
                }
            }
        });
    });
    // The `_type` spelling of each type, which the SDK never writes.
    for field in &mut declaration.entities[0].fields {
        if let Some(base) = field.name.strip_suffix("_alias") {
            field.field_type = format!("{base}_type");
        }
    }
    let build = build([declaration]);

    assert!(
        coded(&build, "W019").is_empty(),
        "{:?}",
        diagnostics(&build)
    );
    for field_type in FieldType::ALL {
        let name = field_type.as_str();
        for field in [name.to_string(), format!("{name}_alias")] {
            let entry = build.fields().get("thing", &field).expect(&field);
            assert_eq!(entry.field_type(), *field_type, "{field}");
            assert_eq!(entry.declared().field_type, name, "{field}");
        }
    }
    // An enum keeps its declared values, and its descriptor keeps them too.
    assert_eq!(
        build
            .fields()
            .get("thing", "enum_alias")
            .unwrap()
            .declared()
            .enum_values,
        ["draft", "done"]
    );
}

#[spec(
    behavior = "registry_build_fields",
    verify = "a field of a type the protocol does not define is W019 and not registered"
)]
fn a_field_of_an_undefined_type_is_w019_and_not_registered() {
    let mut declaration = declare("@test/ext", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
            k.field("summary", |f| {
                f.field_type(FieldType::String);
            });
            k.field("notes", |f| {
                f.field_type(FieldType::String);
            });
            k.field("data", |f| {
                f.field_type(FieldType::String);
            });
        });
    });
    // Neither is a field type the protocol defines.
    declaration.entities[0].fields[1].field_type = "text".into();
    declaration.entities[0].fields[2].field_type = "unknown_type_xyz".into();
    let build = build([declaration]);

    let w019 = coded(&build, "W019");
    assert_eq!(w019.len(), 2, "{:?}", diagnostics(&build));
    assert_eq!(codes(&build), ["W019", "W019"], "nothing but the two W019s");
    for (diagnostic, quoted) in w019.iter().zip(["'text'", "'unknown_type_xyz'"]) {
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert!(
            diagnostic.message.contains(quoted),
            "{}",
            diagnostic.message
        );
    }
    assert!(!build.fields().contains("thing", "notes"));
    assert!(!build.fields().contains("thing", "data"));
    assert!(build.fields().contains("thing", "summary"));
    assert!(build.kinds().contains("thing"));
}

#[spec(
    behavior = "registry_build_fields",
    verify = "a registry entry is built only from a descriptor whose type the host reads"
)]
fn an_entry_is_built_only_from_a_type_the_host_reads() {
    let described = |field_type: &str| FieldDescriptor {
        name: "level".into(),
        field_type: field_type.into(),
        ..Default::default()
    };

    assert_eq!(
        FieldRegistryEntry::new("thing", "@t/x", described("prose")),
        Err(UnknownFieldType("prose".into()))
    );

    let unknown_role = FieldDescriptor {
        proof_role: Some("assumed".into()),
        ..described("string")
    };
    let entry = FieldRegistryEntry::new("thing", "@t/x", unknown_role).unwrap();
    assert_eq!(entry.proof_role(), None);

    let level = FieldDescriptor {
        enum_values: vec!["low".into(), "high".into()],
        ..described("enum")
    };
    let entry = FieldRegistryEntry::new("thing", "@t/x", level).unwrap();
    assert_eq!(entry.field_type(), FieldType::Enum);
    assert_eq!(entry.enum_values(), ["low", "high"]);
    assert_eq!(entry.type_label(), "enum (low, high)");

    // A string field declaring enum values has no enum values.
    let not_an_enum = FieldDescriptor {
        enum_values: vec!["low".into()],
        ..described("string")
    };
    let entry = FieldRegistryEntry::new("thing", "@t/x", not_an_enum).unwrap();
    assert_eq!(entry.field_type(), FieldType::String);
    assert!(entry.enum_values().is_empty());
    assert_eq!(entry.type_label(), "string");
}

#[spec(
    behavior = "registry_build_fields",
    verify = "a registry entry names its type canonically, whatever spelling was declared"
)]
fn an_entry_names_its_type_canonically() {
    for spelling in ["boolean", "bool_type", "bool"] {
        let declared = FieldDescriptor {
            name: "urgent".into(),
            field_type: spelling.into(),
            ..Default::default()
        };
        let entry = FieldRegistryEntry::new("thing", "@t/x", declared).unwrap();
        assert_eq!(entry.declared().field_type, "bool", "{spelling}");
        assert_eq!(entry.field_type(), FieldType::Bool, "{spelling}");
    }
}

#[spec(
    behavior = "registry_build_fields",
    verify = "a field's normative flag reaches its registry entry"
)]
fn a_fields_normative_flag_reaches_its_entry() {
    let build = build([declare("@test/ext", |c| {
        c.kind("Rule", |k| {
            k.keyword("rule");
            k.field("guarantee", |f| {
                f.field_type(FieldType::String).normative();
            });
            k.field("description", |f| {
                f.field_type(FieldType::String);
            });
        });
    })]);
    let normative = |field: &str| {
        build
            .fields()
            .get("rule", field)
            .unwrap()
            .declared()
            .normative
    };
    assert!(normative("guarantee"));
    assert!(!normative("description"));
}

/// An extension whose `rule` kind declares `limit` (a kind field) and whose
/// shared `goal` field reaches every kind, with the roles given.
fn roles_extension(limit_role: &str, goal_role: &str) -> ExtensionDeclaration {
    declare("@test/ext", |c| {
        c.shared_field("goal", |f| {
            f.field_type(FieldType::String).proof_role(goal_role);
        });
        c.kind("Rule", |k| {
            k.keyword("rule");
            k.field("limit", |f| {
                f.field_type(FieldType::String).proof_role(limit_role);
            });
            k.field("description", |f| {
                f.field_type(FieldType::String);
            });
        });
    })
}

#[spec(
    behavior = "registry_build_fields",
    verify = "a field's proof_role reaches the field registry"
)]
fn a_fields_proof_role_reaches_its_entry() {
    // An enhancement's field carries its role onto the target kind.
    let other = declare("@test/other", |c| {
        c.enhance("rule", "@test/ext", |e| {
            e.field("expression", |f| {
                f.field_type(FieldType::String).proof_role("claim");
            });
        });
    });
    let build = build([roles_extension("bound", "claim"), other]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    let role = |field: &str| build.fields().get("rule", field).unwrap().proof_role();
    assert_eq!(role("limit"), Some(ProofRole::Bound));
    assert_eq!(role("goal"), Some(ProofRole::Claim));
    assert_eq!(role("description"), None);
    assert_eq!(role("expression"), Some(ProofRole::Claim));
    assert_eq!(
        build
            .fields()
            .get("rule", "expression")
            .unwrap()
            .source_extension(),
        "@test/ext",
        "an enhancement field is the kind's through its owner"
    );
}

#[spec(
    behavior = "registry_build_fields",
    verify = "a proof_role other than bound or claim is refused"
)]
fn a_proof_role_other_than_bound_or_claim_is_refused() {
    let build = build([roles_extension("assumed", "claim")]);

    let refused = coded(&build, "W021");
    assert_eq!(refused.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(refused[0].severity, Severity::Warning);
    assert!(
        refused[0].message.contains("proof_role 'assumed'"),
        "{}",
        refused[0].message
    );
    assert!(
        refused[0].message.contains("'limit'"),
        "{}",
        refused[0].message
    );
    // The field is registered; it has no role.
    let limit = build.fields().get("rule", "limit").unwrap();
    assert_eq!(limit.proof_role(), None);
    assert_eq!(limit.declared().proof_role.as_deref(), Some("assumed"));
    assert_eq!(
        build.fields().get("rule", "goal").unwrap().proof_role(),
        Some(ProofRole::Claim)
    );
}

#[spec(
    behavior = "registry_build_fields",
    verify = "an enum field without values, or whose default_value is not one of its values, is W021 and still registered"
)]
fn an_enum_without_values_or_with_a_default_outside_them_is_w021() {
    let mut declaration = declare("@test/ext", |c| {
        c.kind("Thing", |k| {
            k.keyword("thing");
            for name in ["bare", "defaulted", "fine"] {
                k.field(name, |f| {
                    f.field_type(FieldType::String);
                });
            }
        });
    });
    // The builder cannot say an enum without values or with a stray default.
    for field in &mut declaration.entities[0].fields {
        field.field_type = "enum".to_string();
        match field.name.as_str() {
            "defaulted" => {
                field.enum_values = vec!["a".to_string(), "b".to_string()];
                field.default_value = Some("c".to_string());
            }
            "fine" => {
                field.enum_values = vec!["a".to_string(), "b".to_string()];
                field.default_value = Some("b".to_string());
            }
            _ => {}
        }
    }
    let build = build([declaration]);

    let messages: Vec<&str> = coded(&build, "W021")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "extension '@test/ext': field 'bare' on kind 'thing' is an enum that declares no values",
            "extension '@test/ext': field 'defaulted' on kind 'thing' declares default_value 'c', which is not one of its values: a, b",
        ],
        "{:?}",
        diagnostics(&build)
    );
    for name in ["bare", "defaulted", "fine"] {
        assert!(build.fields().contains("thing", name), "{name}");
    }
}

#[spec(
    behavior = "registry_build_fields",
    verify = "Registry Build Registers Fields: field registration holds — declarations_in_load_order, fields_registered, types_parsed, roles_checked, enums_checked"
)]
fn field_registration_holds() {
    // declarations_in_load_order: software, then an extension with a field
    // of an unknown type and one with a bad role.
    let mut late = declare("@test/late", |c| {
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("title_text", |f| {
                f.field_type(FieldType::String);
            });
            k.field("bound", |f| {
                f.field_type(FieldType::Integer).proof_role("bound");
            });
            k.field("odd", |f| {
                f.field_type(FieldType::String).proof_role("maybe");
            });
        });
    });
    late.entities[0].fields[0].field_type = "prose".into();
    let build = build([software(), late]);

    // fields_registered: every declared field, embedding its descriptor.
    let contract = build.fields().get("behavior", "contract").unwrap();
    assert_eq!(contract.field_type(), FieldType::Block);
    assert_eq!(contract.declared().name, "contract");
    let invariants = build.fields().get("behavior", "invariants").unwrap();
    assert_eq!(invariants.field_type(), FieldType::ReferenceList);
    assert_eq!(invariants.declared().edge.as_deref(), Some("enforces"));
    assert_eq!(
        invariants.declared().target_kind.as_deref(),
        Some("invariant")
    );

    // types_parsed: the unknown type is W019 and not registered.
    let w019 = coded(&build, "W019");
    assert_eq!(w019.len(), 1);
    assert!(w019[0].message.contains("'prose'"));
    assert!(!build.fields().contains("task", "title_text"));

    // enums_checked: an_enum_without_values_or_with_a_default_outside_them_is_w021.
    // roles_checked: the bad role is W021 and the field has none.
    assert_eq!(
        build.fields().get("task", "bound").unwrap().proof_role(),
        Some(ProofRole::Bound)
    );
    assert_eq!(
        build.fields().get("task", "odd").unwrap().proof_role(),
        None
    );
    let w021 = coded(&build, "W021");
    assert_eq!(w021.len(), 1);
    assert!(w021[0].message.contains("proof_role 'maybe'"));
    assert!(
        diagnostics(&build)
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        diagnostics(&build)
    );
}
