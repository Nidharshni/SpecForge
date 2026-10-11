//! `registry_build_edges`: every declared edge type, and every edge label a
//! field maps to without declaring it, is in the build's `EdgeRegistry`.

use specforge_common::Severity;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::{EdgeTypeDescriptor, ExtensionDeclaration};
use specforge_test_macros::test as spec;

use crate::support::{build, coded, codes, declare, diagnostics, product, software};

/// `@specforge/product` declaring the edge type `enforces` again: from
/// `feature` to `behavior`, dotted.
fn product_redeclaring_enforces() -> ExtensionDeclaration {
    let mut product = product();
    product.edges.push(EdgeTypeDescriptor {
        label: "enforces".to_string(),
        source_kind: Some("feature".to_string()),
        target_kind: Some("behavior".to_string()),
        edge_style: Some("dotted".to_string()),
        ..Default::default()
    });
    product
}

#[spec(
    behavior = "registry_build_edges",
    verify = "edge type registered with label and description"
)]
fn an_edge_type_is_registered_with_its_label_and_description() {
    let declared = declare("@test/ext", |c| {
        c.kind("A", |k| {
            k.keyword("a");
        });
        c.kind("B", |k| {
            k.keyword("b");
        });
        c.edge("guards", |e| {
            e.description("A guards the B it names")
                .source_kind("a")
                .target_kind("b");
        });
        c.edge("touches", |e| {
            e.source_kind("a").target_kind("b");
        });
    });
    let build = build([software(), declared]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    let guards = build.edges().get("guards").unwrap();
    assert_eq!(guards.declared.label, "guards");
    assert_eq!(
        guards.declared.description.as_deref(),
        Some("A guards the B it names")
    );
    assert_eq!(guards.source_extension, "@test/ext");
    let touches = build.edges().get("touches").unwrap();
    assert_eq!(touches.declared.label, "touches");
    assert_eq!(touches.declared.description, None);
    // An explicit edge type of another extension, merged into the same set.
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(enforces.declared.label, "enforces");
    assert_eq!(enforces.source_extension, "@specforge/software");
    assert_eq!(build.edges().len(), 3);
}

#[spec(
    behavior = "registry_build_edges",
    verify = "source/target kind constraints recorded"
)]
fn an_edge_types_kind_constraints_are_recorded() {
    let build = build([software()]);
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(enforces.declared.source_kind.as_deref(), Some("behavior"));
    assert_eq!(enforces.declared.target_kind.as_deref(), Some("invariant"));
    assert_eq!(enforces.declared.edge_style.as_deref(), Some("dashed"));
}

#[spec(
    behavior = "registry_build_edges",
    verify = "a field's edge label no edge type declares is registered as an edge from the field's kind to its target kind"
)]
fn a_fields_undeclared_edge_label_is_registered_as_an_edge() {
    let declared = declare("@test/ext", |c| {
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("assignee", |f| {
                f.field_type(FieldType::Reference)
                    .edge("assigned_to")
                    .target_kind("person");
            });
            k.field("owner", |f| {
                f.field_type(FieldType::Reference).edge("owns");
            });
        });
        c.kind("Person", |k| {
            k.keyword("person");
        });
    });
    let build = build([software(), product(), declared]);

    let assigned = build.edges().get("assigned_to").expect("implicit edge");
    assert_eq!(assigned.declared.label, "assigned_to");
    assert_eq!(assigned.source_kinds(), ["task"]);
    assert_eq!(assigned.target_kinds(), ["person"]);
    assert_eq!(assigned.source_extension, "@test/ext");
    // A label whose field names no target kind: an edge to no kind.
    let owns = build.edges().get("owns").expect("implicit edge");
    assert_eq!(owns.source_kinds(), ["task"]);
    assert!(owns.target_kinds().is_empty());
    // A label an edge type declares keeps the declared one.
    let composes = build.edges().get("composes").unwrap();
    assert_eq!(composes.source_extension, "@specforge/product");
    assert_eq!(composes.declared.source_kind.as_deref(), Some("feature"));
    // The undeclared labels are the extension's authoring errors (W021),
    // not a reason to leave the edges out.
    assert_eq!(coded(&build, "W021").len(), 2, "{:?}", diagnostics(&build));
}

#[spec(
    behavior = "registry_build_edges",
    verify = "an edge label several kinds write without declaring it names every writer, in load order"
)]
fn an_undeclared_edge_label_several_kinds_write_names_every_writer() {
    let first = || {
        declare("@test/first", |c| {
            c.kind("Task", |k| {
                k.keyword("task");
                k.field("assignee", |f| {
                    f.field_type(FieldType::Reference)
                        .edge("assigned_to")
                        .target_kind("person");
                });
            });
            c.kind("Person", |k| {
                k.keyword("person");
            });
        })
    };
    let second = declare("@test/second", |c| {
        c.kind("Ticket", |k| {
            k.keyword("ticket");
            k.field("owner", |f| {
                f.field_type(FieldType::Reference).edge("assigned_to");
            });
            k.field("team", |f| {
                f.field_type(FieldType::Reference)
                    .edge("assigned_to")
                    .target_kind("team");
            });
        });
        c.kind("Team", |k| {
            k.keyword("team");
        });
    });
    let build = build([first(), second]);

    let assigned = build.edges().get("assigned_to").expect("implicit edge");
    let writers: Vec<(&str, &str, &str, Option<&str>)> = assigned
        .writers
        .iter()
        .map(|w| {
            (
                w.extension.as_str(),
                w.kind.as_str(),
                w.field.as_str(),
                w.target_kind.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        writers,
        [
            ("@test/first", "task", "assignee", Some("person")),
            ("@test/second", "ticket", "owner", None),
            ("@test/second", "ticket", "team", Some("team")),
        ]
    );
    // Its ends are every writer's, each once, in load order; the edge type
    // itself declares neither.
    assert_eq!(assigned.source_kinds(), ["task", "ticket"]);
    assert_eq!(assigned.target_kinds(), ["person", "team"]);
    assert_eq!(assigned.declared.source_kind, None);
    assert_eq!(assigned.declared.target_kind, None);
    // The first writer in load order owns it.
    assert_eq!(assigned.source_extension, "@test/first");
    assert!(
        coded(&build, "W018").is_empty(),
        "{:?}",
        diagnostics(&build)
    );

    // A label a later extension declares is that declaration, whoever
    // wrote it first without declaring it: no W018, no writers.
    let declaring = declare("@test/declaring", |c| {
        c.edge("assigned_to", |e| {
            e.source_kind("person").target_kind("task");
        });
    });
    let declared = crate::support::build([first(), declaring]);
    let assigned = declared.edges().get("assigned_to").unwrap();
    assert_eq!(assigned.source_extension, "@test/declaring");
    assert!(assigned.writers.is_empty());
    assert_eq!(assigned.source_kinds(), ["person"]);
    assert_eq!(assigned.target_kinds(), ["task"]);
    assert!(coded(&declared, "W018").is_empty());
}

#[spec(
    behavior = "registry_build_edges",
    verify = "an edge label a later extension declares again is W018 and the first in load order keeps it"
)]
fn an_edge_label_declared_twice_is_w018_and_the_first_keeps_it() {
    assert!(coded(&build([software(), product()]), "W018").is_empty());

    let build = build([software(), product_redeclaring_enforces()]);
    let w018 = coded(&build, "W018");
    assert_eq!(w018.len(), 1, "{:?}", diagnostics(&build));
    assert_eq!(w018[0].severity, Severity::Warning);
    assert_eq!(
        w018[0].message,
        "edge type 'enforces' from '@specforge/product' duplicates 'enforces' from '@specforge/software' (first wins)"
    );
    assert_eq!(codes(&build), ["W018"]);
    // The first declaration keeps the label and its constraints.
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(enforces.source_extension, "@specforge/software");
    assert_eq!(enforces.declared.edge_style.as_deref(), Some("dashed"));
    assert_eq!(enforces.declared.source_kind.as_deref(), Some("behavior"));
    assert_eq!(enforces.declared.target_kind.as_deref(), Some("invariant"));

    // A label two extensions declare with nothing else: still one W018.
    let mut first = software();
    let mut second = product();
    for declaration in [&mut first, &mut second] {
        declaration.edges.push(EdgeTypeDescriptor {
            label: "links_to".to_string(),
            ..Default::default()
        });
    }
    let links = crate::support::build([first, second]);
    let w018 = coded(&links, "W018");
    assert_eq!(w018.len(), 1);
    assert!(w018[0].message.contains("'links_to'"));
    assert_eq!(
        links.edges().get("links_to").unwrap().source_extension,
        "@specforge/software"
    );
}

#[spec(
    behavior = "registry_build_edges",
    verify = "Registry Build Registers Edge Types: edge type registration holds — declarations_in_load_order, edge_types_registered, every_writer_named, first_wins"
)]
fn edge_registration_holds() {
    // declarations_in_load_order: software, then product, which declares
    // `enforces` again.
    let build = build([software(), product_redeclaring_enforces()]);

    // edge_types_registered: both extensions' edge types.
    let mut labels: Vec<&str> = build.edges().labels().map(String::as_str).collect();
    labels.sort();
    assert_eq!(labels, ["composes", "enforces"]);
    assert_eq!(
        build.edges().get("composes").unwrap().source_extension,
        "@specforge/product"
    );

    // every_writer_named: an edge label two kinds write without declaring
    // it names both, in load order.
    let writes = |name: &str, kind: &str| {
        let kind = kind.to_string();
        declare(name, move |c| {
            c.kind(&kind, |k| {
                k.field("links", |f| {
                    f.field_type(FieldType::ReferenceList).edge("linked");
                });
            });
        })
    };
    let linked = crate::support::build([writes("@test/a", "alpha"), writes("@test/b", "beta")]);
    let linked = linked.edges().get("linked").unwrap();
    assert_eq!(linked.source_kinds(), ["alpha", "beta"]);
    assert_eq!(linked.writers.len(), 2);

    // first_wins: W018, and the later constraints are discarded.
    assert_eq!(coded(&build, "W018").len(), 1);
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(enforces.source_extension, "@specforge/software");
    assert_eq!(enforces.declared.source_kind.as_deref(), Some("behavior"));
    assert!(
        diagnostics(&build)
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        diagnostics(&build)
    );
}
