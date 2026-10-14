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
    assert_eq!(guards.label(), "guards");
    assert_eq!(
        guards.declared().unwrap().description.as_deref(),
        Some("A guards the B it names")
    );
    assert_eq!(guards.source_extension(), "@test/ext");
    let touches = build.edges().get("touches").unwrap();
    assert_eq!(touches.label(), "touches");
    assert_eq!(touches.declared().unwrap().description, None);
    // An explicit edge type of another extension, merged into the same set.
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(enforces.label(), "enforces");
    assert_eq!(enforces.source_extension(), "@specforge/software");
    assert_eq!(build.edges().len(), 3);
}

#[spec(
    behavior = "registry_build_edges",
    verify = "source/target kind constraints recorded"
)]
fn an_edge_types_kind_constraints_are_recorded() {
    let build = build([software()]);
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(
        enforces.declared().unwrap().source_kind.as_deref(),
        Some("behavior")
    );
    assert_eq!(
        enforces.declared().unwrap().target_kind.as_deref(),
        Some("invariant")
    );
    assert_eq!(
        enforces.declared().unwrap().edge_style.as_deref(),
        Some("dashed")
    );
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
    assert_eq!(assigned.label(), "assigned_to");
    assert_eq!(assigned.sources(), ["task"]);
    assert_eq!(assigned.targets(), ["person"]);
    assert_eq!(assigned.source_extension(), "@test/ext");
    // A label whose field names no target kind: an edge to no kind.
    let owns = build.edges().get("owns").expect("implicit edge");
    assert_eq!(owns.sources(), ["task"]);
    assert!(owns.targets().is_empty());
    // A label an edge type declares keeps the declared one.
    let composes = build.edges().get("composes").unwrap();
    assert_eq!(composes.source_extension(), "@specforge/product");
    assert_eq!(
        composes.declared().unwrap().source_kind.as_deref(),
        Some("feature")
    );
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
        .writers()
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
    assert_eq!(assigned.sources(), ["task", "ticket"]);
    assert_eq!(assigned.targets(), ["person", "team"]);
    assert!(assigned.declared().is_none());
    // The first writer in load order owns it.
    assert_eq!(assigned.source_extension(), "@test/first");
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
    assert_eq!(assigned.source_extension(), "@test/declaring");
    assert!(assigned.writers().is_empty());
    assert_eq!(assigned.sources(), ["person"]);
    assert_eq!(assigned.targets(), ["task"]);
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
    assert_eq!(enforces.source_extension(), "@specforge/software");
    assert_eq!(
        enforces.declared().unwrap().edge_style.as_deref(),
        Some("dashed")
    );
    assert_eq!(
        enforces.declared().unwrap().source_kind.as_deref(),
        Some("behavior")
    );
    assert_eq!(
        enforces.declared().unwrap().target_kind.as_deref(),
        Some("invariant")
    );

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
        links.edges().get("links_to").unwrap().source_extension(),
        "@specforge/software"
    );
}

#[spec(
    behavior = "registry_build_edges",
    verify = "Registry Build Registers Edge Types: edge type registration holds — declarations_in_load_order, edge_types_registered, every_writer_named, one_type_per_field, first_wins"
)]
fn edge_registration_holds() {
    // declarations_in_load_order: software, then product, which declares
    // `enforces` again.
    let build = build([software(), product_redeclaring_enforces()]);

    // edge_types_registered: both extensions' edge types.
    let mut labels: Vec<&str> = build.edges().iter().map(|e| e.label()).collect();
    labels.sort();
    assert_eq!(labels, ["composes", "enforces"]);
    assert_eq!(
        build.edges().get("composes").unwrap().source_extension(),
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
    let linked_build =
        crate::support::build([writes("@test/a", "alpha"), writes("@test/b", "beta")]);
    let linked = linked_build.edges().get("linked").unwrap();
    assert_eq!(linked.sources(), ["alpha", "beta"]);
    assert_eq!(linked.writers().len(), 2);

    // one_type_per_field: the build answers one edge type for each field
    // on its kind, and none for a field that writes no edge.
    for kind in ["alpha", "beta"] {
        assert_eq!(
            linked_build
                .edges()
                .written_by(kind, "links")
                .map(|e| e.label()),
            Some("linked")
        );
        assert!(linked_build.edges().written_by(kind, "absent").is_none());
    }

    // first_wins: W018, and the later constraints are discarded.
    assert_eq!(coded(&build, "W018").len(), 1);
    let enforces = build.edges().get("enforces").unwrap();
    assert_eq!(enforces.source_extension(), "@specforge/software");
    assert_eq!(
        enforces.declared().unwrap().source_kind.as_deref(),
        Some("behavior")
    );
    assert!(
        diagnostics(&build)
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        diagnostics(&build)
    );
}

#[spec(
    behavior = "registry_build_edges",
    verify = "an edge type names every field that writes it: a kind's own, shared and enhancement fields alike"
)]
fn a_declared_edge_type_names_its_writers() {
    let declared = declare("@test/ext", |c| {
        c.kind("Task", |k| {
            k.keyword("task");
            k.field("owner", |f| {
                f.field_type(FieldType::Reference)
                    .edge("owns")
                    .target_kind("person");
            });
            k.field("title", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.kind("Person", |k| {
            k.keyword("person");
        });
        c.shared_field("watches", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("owns")
                .target_kind("person");
        });
        c.enhance("person", "@test/ext", |e| {
            e.field("friend_of", |f| {
                f.field_type(FieldType::ReferenceList)
                    .edge("owns")
                    .target_kind("person");
            });
        });
        c.edge("owns", |e| {
            e.target_kind("person");
        });
    });
    let build = build([software(), declared]);
    assert!(diagnostics(&build).is_empty(), "{:?}", diagnostics(&build));

    // software's `enforces` is written by the one field that names it.
    let enforces = build.edges().get("enforces").unwrap();
    let writers = |entry: &specforge_registry::EdgeRegistryEntry| -> Vec<String> {
        entry
            .writers()
            .iter()
            .map(|w| format!("{}.{}", w.kind, w.field))
            .collect()
    };
    assert_eq!(writers(enforces), ["behavior.invariants"]);

    // A kind's own field, a shared field (on every kind) and an
    // enhancement's field are all writers, by kind then field.
    let owns = build.edges().get("owns").unwrap();
    assert_eq!(
        writers(owns),
        [
            "person.friend_of",
            "person.watches",
            "task.owner",
            "task.watches"
        ]
    );
    assert!(owns.writers().iter().all(|w| w.extension == "@test/ext"));
    // Its source end is open, so its writers' kinds are its sources; its
    // target end is the declared one.
    assert_eq!(owns.sources(), ["person", "task"]);
    assert_eq!(owns.targets(), ["person"]);

    // `written_by` answers each field of each kind; a field with no edge
    // writes none.
    let label = |kind: &str, field: &str| {
        build
            .edges()
            .written_by(kind, field)
            .map(|e| e.label().to_string())
    };
    assert_eq!(label("task", "owner").as_deref(), Some("owns"));
    assert_eq!(label("task", "watches").as_deref(), Some("owns"));
    assert_eq!(label("person", "friend_of").as_deref(), Some("owns"));
    assert_eq!(label("behavior", "invariants").as_deref(), Some("enforces"));
    assert_eq!(label("task", "title"), None);
    assert_eq!(label("task", "no_such_field"), None);
}

#[spec(
    behavior = "registry_build_edges",
    verify = "a field that writes a declared edge type from a kind outside its declared source kind, or to a kind outside its declared target kind, is W021 and writes no edge of that type"
)]
fn a_writer_outside_a_declared_end_is_w021_and_writes_nothing() {
    let declared = declare("@test/ext", |c| {
        c.kind("Job", |k| {
            k.keyword("job");
            k.field("a", |f| {
                f.field_type(FieldType::Reference)
                    .edge("relates")
                    .target_kind("person");
            });
            k.field("b", |f| {
                f.field_type(FieldType::Reference)
                    .edge("relates")
                    .target_kind("job");
            });
            // A writer with no declared target is admitted.
            k.field("d", |f| {
                f.field_type(FieldType::Reference).edge("relates");
            });
        });
        c.kind("Thing", |k| {
            k.keyword("thing");
            k.field("c", |f| {
                f.field_type(FieldType::Reference)
                    .edge("relates")
                    .target_kind("person");
            });
        });
        c.kind("Person", |k| {
            k.keyword("person");
        });
        c.edge("relates", |e| {
            e.source_kind("job").target_kind("person");
        });
    });
    let build = build([declared]);

    let messages: Vec<&str> = coded(&build, "W021")
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "extension '@test/ext': field 'b' on kind 'job' writes edge type 'relates', whose declared target_kind is 'person', not 'job' — its edges are not of that type",
            "extension '@test/ext': field 'c' on kind 'thing' writes edge type 'relates', whose declared source_kind is 'job' — its edges are not of that type",
        ]
    );
    let relates = build.edges().get("relates").unwrap();
    let writers: Vec<&str> = relates.writers().iter().map(|w| w.field.as_str()).collect();
    assert_eq!(writers, ["a", "d"]);
    assert!(build.edges().written_by("job", "b").is_none());
    assert!(build.edges().written_by("thing", "c").is_none());
    assert!(build.edges().written_by("job", "a").is_some());
    // The declared ends stay the edge type's ends.
    assert_eq!(relates.sources(), ["job"]);
    assert_eq!(relates.targets(), ["person"]);
}
