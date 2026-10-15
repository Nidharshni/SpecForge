//! `registry_build_declaration_consistency`: each declaration's references,
//! checked by structure alone against the loaded declarations. A reference
//! that does not resolve is a W021 warning among the declaration
//! diagnostics; it never keeps the declaration's kinds out.

use specforge_common::Severity;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{EdgeTypeDescriptor, ExtensionDeclaration, PeerDependency};
use specforge_test_macros::test as spec;

use crate::support::{
    build, coded, codes, declare, diagnostics, extension, optional_peer, peer, product, software,
};

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a target_kind the extension or a loaded peer declares passes"
)]
fn a_target_kind_the_extension_or_a_loaded_peer_declares_passes() {
    // Its own kind.
    let own = declare("@test/own", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("invariants", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("invariant");
            });
        });
        c.kind("Invariant", |k| {
            k.keyword("invariant");
        });
    });
    let build = build([own]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    // A kind of a peer that is loaded.
    let needs_software = || {
        let mut c = extension("@test/ext");
        c.meta
            .peer_dependencies
            .push(peer("@specforge/software", ">=1.0.0"));
        c.kind("Feature", |k| {
            k.keyword("feature_like");
            k.field("behaviors", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("behavior");
            });
        });
        c.declaration()
    };
    let with_peer = crate::support::build([software(), needs_software()]);
    assert!(
        diagnostics(&with_peer).is_empty(),
        "{:?}",
        diagnostics(&with_peer)
    );
    assert!(with_peer.kinds().contains("feature_like"));

    // A named peer that is not loaded: its kinds are unknown, so the target
    // is let through (the missing peer is E027, not W021).
    let without_peer = crate::support::build([needs_software()]);
    assert!(coded(&without_peer, "W021").is_empty());
    assert_eq!(codes(&without_peer), ["E027"]);
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "an edge label the extension declares an edge type for passes"
)]
fn an_edge_label_the_extension_declares_passes() {
    let build = build([software(), product()]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    let invariants = build.fields().get("behavior", "invariants").unwrap();
    assert_eq!(invariants.declared().edge.as_deref(), Some("enforces"));
    let behaviors = build.fields().get("feature", "behaviors").unwrap();
    assert_eq!(behaviors.declared().edge.as_deref(), Some("composes"));
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a target_kind no loaded extension declares is a W021 warning"
)]
fn a_target_kind_no_loaded_extension_declares_is_w021() {
    let build = build([declare("@test/tasks", |c| {
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("owner", |f| {
                f.field_type(FieldType::Reference).target_kind("robot");
            });
        });
    })]);

    let w021 = coded(&build, "W021");
    assert_eq!(w021.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(w021[0].severity, Severity::Warning);
    for part in [
        "'@test/tasks'",
        "'owner'",
        "'task'",
        "target_kind 'robot'",
        "not declared by this extension",
    ] {
        assert!(w021[0].message.contains(part), "{}", w021[0].message);
    }
    assert_eq!(codes(&build), ["W021"]);
    assert!(build.kinds().contains("task"));
    assert!(build.fields().contains("task", "owner"));
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a target_kind only a non-peer extension declares is a W021 warning naming that extension"
)]
fn a_target_kind_only_a_non_peer_declares_is_w021_naming_it() {
    // A peer of @specforge/software only, yet it points at product's
    // `feature`.
    let mut c = extension("@test/ext");
    c.meta
        .peer_dependencies
        .push(peer("@specforge/software", ">=1.0.0"));
    c.kind("Task", |k| {
        k.keyword("task");
        k.field("behaviors", |f| {
            f.field_type(FieldType::ReferenceList)
                .target_kind("behavior");
        });
        k.field("features", |f| {
            f.field_type(FieldType::ReferenceList)
                .target_kind("feature");
        });
    });
    let build = build([software(), product(), c.declaration()]);

    // `behavior` comes from the peer and passes; `feature` exists, but only
    // in an extension this one does not depend on.
    let w021 = coded(&build, "W021");
    assert_eq!(w021.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(diagnostics(&build).len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(w021[0].severity, Severity::Warning);
    assert_eq!(
        w021[0].message,
        "extension '@test/ext': field 'features' on kind 'task' references target_kind 'feature' declared by '@specforge/product', which is not a peer dependency"
    );
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "an edge label the extension declares no edge type for is a W021 warning"
)]
fn an_edge_label_with_no_declared_edge_type_is_w021() {
    let build = build([declare("@test/ext", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("invariants", |f| {
                f.field_type(FieldType::ReferenceList).edge("missing_edge");
            });
        });
    })]);

    let w021 = coded(&build, "W021");
    assert_eq!(w021.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(w021[0].severity, Severity::Warning);
    for part in ["'invariants'", "edge label 'missing_edge'"] {
        assert!(w021[0].message.contains(part), "{}", w021[0].message);
    }
    assert_eq!(codes(&build), ["W021"]);
    // The label is still registered, as an edge of the field's kind.
    assert!(build.edges().contains("missing_edge"));
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a derived_from the host can't apply produces a W021 warning"
)]
fn a_derived_from_the_host_cannot_apply_is_w021() {
    let build = build([declare("@test/ext", |c| {
        c.kind("Shape", |k| {
            k.keyword("shape");
            k.field("parts", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("type_expressions");
            });
            k.field("uses", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("method_signatures");
            });
            k.field("guessed", |f| {
                f.field_type(FieldType::ReferenceList)
                    .target_kind("shape")
                    .derived_from("comments");
            });
            k.field("untargeted", |f| {
                f.field_type(FieldType::ReferenceList)
                    .derived_from("type_expressions");
            });
            k.field("note", |f| {
                f.field_type(FieldType::String)
                    .target_kind("shape")
                    .derived_from("type_expressions");
            });
        });
    })]);

    let w021 = coded(&build, "W021");
    let named: Vec<&str> = ["'parts'", "'uses'", "'guessed'", "'untargeted'", "'note'"]
        .into_iter()
        .filter(|field| w021.iter().any(|d| d.message.contains(field)))
        .collect();
    assert_eq!(named, ["'guessed'", "'untargeted'", "'note'"], "{w021:?}");
    assert_eq!(codes(&build), ["W021", "W021", "W021"]);
    assert!(
        w021.iter()
            .all(|d| d.severity == Severity::Warning && d.message.contains("derived_from")),
        "{w021:?}"
    );
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "cross-validation uses no domain-specific logic"
)]
fn consistency_is_checked_by_structure_alone() {
    // A domain the host knows nothing of: its references resolve, so
    // nothing is reported.
    let build = build([declare("@custom/cooking", |c| {
        c.kind("Recipe", |k| {
            k.keyword("recipe");
            k.field("ingredients", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("uses")
                    .target_kind("ingredient");
            });
        });
        c.kind("Ingredient", |k| {
            k.keyword("ingredient");
        });
        c.edge("uses", |e| {
            e.source_kind("recipe").target_kind("ingredient");
        });
    })]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));
    assert!(build.kinds().contains("recipe") && build.kinds().contains("ingredient"));
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "Registry Build Checks Declaration Consistency: declaration consistency holds — declarations_in_load_order, peers_loaded, references_resolved, authoring_errors_diagnosed, compile_not_failed"
)]
fn declaration_consistency_holds() {
    // declarations_in_load_order + peers_loaded: the extension loads before
    // its peer; the check waits for both.
    let mut c = extension("@test/tasks");
    c.meta
        .peer_dependencies
        .push(peer("@test/people", "^1.0.0"));
    c.kind("Task", |k| {
        k.keyword("task");
        k.field("owner", |f| {
            f.field_type(FieldType::Reference)
                .target_kind("person")
                .edge("owned_by");
        });
        k.field("robot", |f| {
            f.field_type(FieldType::Reference).target_kind("robot");
        });
        k.field("reviewer", |f| {
            f.field_type(FieldType::Reference)
                .target_kind("person")
                .edge("reviewed_by");
        });
    });
    c.edge("owned_by", |e| {
        e.source_kind("task").target_kind("person");
    });
    c.edge("haunts", |e| {
        e.source_kind("ghost").target_kind("task");
    });
    let mut people = extension("@test/people");
    people
        .meta
        .peer_dependencies
        .push(peer("@test/absent", "^1"));
    people.kind("Person", |k| {
        k.keyword("person");
    });
    let unnamed = declare("", |_| {});
    let build = build([c.declaration(), people.declaration(), unnamed]);

    // references_resolved: `owner` (a peer's kind, its own edge) passes.
    // authoring_errors_diagnosed: every other reference is W021, naming it.
    let w021: Vec<&str> = coded(&build, "W021")
        .into_iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(w021.len(), 3, "{w021:?}");
    assert!(w021[0].contains("'robot'") && w021[0].contains("target_kind 'robot'"));
    assert!(w021[1].contains("edge label 'reviewed_by'"));
    assert!(w021[2].contains("edge type 'haunts' references source_kind 'ghost'"));
    assert!(!w021.iter().any(|m| m.contains("'owner'")), "{w021:?}");

    // After E030 and before E027, and nothing else.
    assert_eq!(codes(&build), ["E030", "W021", "W021", "W021", "E027"]);

    // compile_not_failed: W021 is a warning, and the kinds still register.
    assert!(
        coded(&build, "W021")
            .iter()
            .all(|d| d.severity == Severity::Warning)
    );
    assert!(build.kinds().contains("task") && build.kinds().contains("person"));
    assert!(build.fields().contains("task", "robot"));
}

/// An extension `name` with `peers`, declaring one untargeted
/// `no_incoming_edges` rule `code` scoped to `edge_type`.
fn edge_rule(name: &str, peers: &[&str], code: &str, edge_type: &str) -> ExtensionDeclaration {
    let mut c = extension(name);
    for p in peers {
        c.meta.peer_dependencies.push(peer(p, ">=1.0.0"));
    }
    c.rule(code, |r| {
        r.severity(ValidationSeverity::Warning)
            .message_template("{id}")
            .check(CheckKind::NoIncomingEdges)
            .edge_type(edge_type);
    });
    c.declaration()
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a rule's edge type that neither its extension nor its peers declare produces W021"
)]
fn a_rules_edge_type_nobody_it_knows_declares_is_w021() {
    // software's own edge, used by an extension that does not peer on it.
    let stranger = edge_rule("@test/stranger", &[], "X100", "enforces");
    // A peer's edge, and the extension's own.
    let peering = edge_rule(
        "@test/peering",
        &["@specforge/software"],
        "X101",
        "enforces",
    );
    let mut own = edge_rule("@test/own", &[], "X102", "links");
    own.edges.push(EdgeTypeDescriptor {
        label: "links".to_string(),
        ..Default::default()
    });
    // While a named peer is not loaded, its edges are unknown: anything goes.
    let waiting = edge_rule("@test/waiting", &["@test/absent"], "X103", "Whatever");

    let build = build([software(), stranger, peering, own, waiting]);

    let w021: Vec<&str> = coded(&build, "W021")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        w021,
        [
            "extension '@test/stranger': rule 'X100' references edge type 'enforces' not declared among its edges or its peers' edges"
        ]
    );
    // Only a warning.
    assert_eq!(coded(&build, "W021")[0].severity, Severity::Warning);
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a rule's target kind that neither its extension nor its peers declare produces W021"
)]
fn a_rules_target_kind_nobody_it_knows_declares_is_w021() {
    let targeting = |name: &str, peers: Vec<PeerDependency>, target: &str| {
        let mut c = extension(name);
        c.meta.peer_dependencies = peers;
        c.rule("X200", |r| {
            r.severity(ValidationSeverity::Warning)
                .message_template("{id}")
                .check(CheckKind::NoIncomingEdges)
                .target_kind(target);
        });
        c.declaration()
    };
    // A stranger's kind, and a kind nobody declares.
    let stranger = targeting("@test/stranger", Vec::new(), "behavior");
    let nobody = targeting("@test/nobody", Vec::new(), "ghost");
    // A loaded peer's kind; a kind of an optional peer that is not loaded.
    let peering = targeting(
        "@test/peering",
        vec![peer("@specforge/software", ">=1.0.0")],
        "behavior",
    );
    let waiting = targeting(
        "@test/waiting",
        vec![optional_peer("@test/absent", ">=1.0.0")],
        "absent_kind",
    );

    let build = build([software(), stranger, nobody, peering, waiting]);

    let w021: Vec<&str> = coded(&build, "W021")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        w021,
        [
            "extension '@test/stranger': rule 'X200' references target_kind 'behavior' declared by '@specforge/software', which is not a peer dependency; name it as the rule's target_extension",
            "extension '@test/nobody': rule 'X200' references target_kind 'ghost' not declared by this extension",
        ]
    );
    // A rule for a kind nobody loaded declares (`ghost`, an absent peer's
    // `absent_kind`) is not registered: it is inert (ADR 0020 D5). The W021
    // above still tells its author.
    assert_eq!(build.rules().len(), 2);
}

/// An extension `name` (no peers) with one rule `X300` on `target_kind` /
/// `edge_type`, naming `target_extension` when given, and one field of its
/// own that references a kind nobody declares (to see its field checks run).
fn naming(
    name: &str,
    target_extension: Option<&str>,
    target_kind: &str,
    edge_type: Option<&str>,
) -> ExtensionDeclaration {
    let mut c = extension(name);
    c.kind("Own", |k| {
        k.keyword("own");
        k.field("link", |f| {
            f.field_type(FieldType::Reference).target_kind("nowhere");
        });
    });
    c.rule("X300", |r| {
        r.severity(ValidationSeverity::Warning)
            .message_template("{id}")
            .check(CheckKind::NoOutgoingEdges)
            .target_kind(target_kind);
        if let Some(extension) = target_extension {
            r.target_extension(extension);
        }
        if let Some(edge_type) = edge_type {
            r.edge_type(edge_type);
        }
    });
    c.declaration()
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "a rule's target_extension, loaded, must declare its target kind and edge type; not loaded, the rule is inert and costs no W021"
)]
fn a_rules_target_extension_resolves_its_kind_and_edge_type() {
    let w021 = |build: &specforge_registry::RegistryBuild| -> Vec<String> {
        coded(build, "W021")
            .iter()
            .map(|d| d.message.clone())
            .collect()
    };
    let field_w021 = "extension '@test/own': field 'link' on kind 'own' references target_kind 'nowhere' not declared by this extension";

    // Loaded and declaring the kind and the edge: nothing but the field's.
    let build_loaded = build([
        software(),
        naming(
            "@test/own",
            Some("@specforge/software"),
            "behavior",
            Some("enforces"),
        ),
    ]);
    assert_eq!(w021(&build_loaded), [field_w021]);
    assert_eq!(
        build_loaded.rules().iter().next().unwrap().describe()["target_extension"],
        "@specforge/software"
    );

    // Not loaded: that rule alone is inert and silent; the extension's own
    // field is still checked.
    let build_absent = build([naming(
        "@test/own",
        Some("@specforge/software"),
        "behavior",
        Some("enforces"),
    )]);
    assert_eq!(w021(&build_absent), [field_w021]);
    assert!(build_absent.rules().is_empty(), "the edge type is unknown");

    // Loaded without the kind or the edge type: W021 for each.
    let build_without = build([
        software(),
        naming(
            "@test/own",
            Some("@specforge/software"),
            "ghost",
            Some("nope"),
        ),
    ]);
    assert_eq!(
        w021(&build_without),
        [
            field_w021.to_string(),
            "extension '@test/own': rule 'X300' references target_kind 'ghost' not declared by '@specforge/software', its target_extension".to_string(),
            "extension '@test/own': rule 'X300' references edge type 'nope' not declared by '@specforge/software', its target_extension".to_string(),
        ]
    );

    // Unset, the kind of a non-peer: W021 naming the owner and suggesting it.
    let build_unset = build([software(), naming("@test/own", None, "behavior", None)]);
    assert_eq!(
        w021(&build_unset),
        [
            field_w021.to_string(),
            "extension '@test/own': rule 'X300' references target_kind 'behavior' declared by '@specforge/software', which is not a peer dependency; name it as the rule's target_extension".to_string(),
        ]
    );
}

#[spec(
    behavior = "registry_build_declaration_consistency",
    verify = "an edge label the extension declares no edge type for is a W021 warning"
)]
fn a_shared_or_enhancement_fields_undeclared_edge_label_is_w021() {
    let build = build([declare("@test/ext", |c| {
        c.kind("Task", |k| {
            k.keyword("task");
        });
        c.shared_field("see_also", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("SeeAlso")
                .target_kind("task");
        });
        c.enhance("task", |e| {
            e.field("pins", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("Pins")
                    .target_kind("task");
            });
        });
    })]);

    let messages: Vec<&str> = coded(&build, "W021")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "extension '@test/ext': shared field 'see_also' references edge label 'SeeAlso' not declared among its edges",
            "extension '@test/ext': enhancement field 'pins' on kind 'task' references edge label 'Pins' not declared among its edges",
        ]
    );
    // Both labels are still edge types of their writers.
    assert!(build.edges().contains("SeeAlso"));
    assert!(build.edges().contains("Pins"));
}
