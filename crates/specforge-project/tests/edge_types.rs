//! An edge type through a real load and compile (ADR 0060): which fields
//! write it, what an edge rule counts and what a cycle rule follows. Each
//! scenario is an in-process SDK extension and a project of a few
//! entities. A pin that still encodes a bug is written as the fixed
//! behaviour and `#[ignore]`d with the ticket that fixes it.

use std::sync::Arc;

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_project::CompiledProject;
use specforge_project::testing::TestProject;
use specforge_test_macros::test as spec;
use specforge_wasm::testing::InProcessRuntime;

/// A kind `name` with one field `field` of type `field_type` writing the
/// edge `edge`, to the kind `target`.
fn kind_writing(
    c: &mut ContributionsBuilder,
    name: &str,
    field: &str,
    field_type: FieldType,
    edge: &str,
    target: &str,
) {
    c.kind(name, |k| {
        k.description(name);
        k.field(field, |f| {
            f.field_type(field_type).edge(edge).target_kind(target);
        });
    });
}

/// A rule `code` (a warning) of `check` over `target`'s entities and the
/// edge type `edge`.
fn edge_rule(
    c: &mut ContributionsBuilder,
    code: &str,
    check: CheckKind,
    target: Option<&str>,
    edge: &str,
    message: &str,
) {
    c.rule(code, |r| {
        r.check(check)
            .edge_type(edge)
            .severity(ValidationSeverity::Warning)
            .message_template(message);
        if let Some(target) = target {
            r.target_kind(target);
        }
    });
}

/// The diagnostics of compiling `source` (`model.spec`) with the extension
/// `name` the builder declares.
fn compile(
    name: &'static str,
    source: &str,
    build: impl Fn(&mut ContributionsBuilder) + Send + Sync + 'static,
) -> Vec<Diagnostic> {
    compiled(name, source, build).diagnostics()
}

/// [`compile`], keeping the compiled project.
fn compiled(
    name: &'static str,
    source: &str,
    build: impl Fn(&mut ContributionsBuilder) + Send + Sync + 'static,
) -> CompiledProject {
    let dir = TestProject::new()
        .config(|c| c["name"] = "pins".into())
        .enabling(&[name])
        .installing_enabled()
        .file("model.spec", source);
    let runtime = InProcessRuntime::new().with(move || {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new(name, "0.1.0"));
        build(&mut c);
        c
    });
    dir.compile_in(Arc::new(runtime))
}

/// The messages of the diagnostics with `code`.
fn messages<'a>(diagnostics: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.as_str())
        .collect()
}

/// `@probe/implicit`: `a` and `b` both write the edge `Owns`, which no
/// extension declares; `W900` asks every `c` for an incoming `Owns`.
fn implicit(c: &mut ContributionsBuilder) {
    for kind in ["a", "b"] {
        kind_writing(c, kind, "owner", FieldType::Reference, "Owns", "c");
    }
    c.kind("c", |k| {
        k.description("owned");
    });
    edge_rule(
        c,
        "W900",
        CheckKind::NoIncomingEdges,
        Some("c"),
        "Owns",
        "c '{id}' is owned by nothing",
    );
}

/// `@probe/declared`: `a` and `b` write `Owns` (declared, target `c`, no
/// source), `d` writes `Mentions`; `W900` asks every `c` for an incoming
/// `Owns`.
fn declared(c: &mut ContributionsBuilder) {
    implicit(c);
    kind_writing(
        c,
        "d",
        "mentions",
        FieldType::ReferenceList,
        "Mentions",
        "c",
    );
    c.edge("Owns", |e| {
        e.target_kind("c");
    });
    c.edge("Mentions", |e| {
        e.source_kind("d").target_kind("c");
    });
}

/// `@probe/pair`: `a.owner` writes `Owns` and `a.likes` writes `Likes`, both
/// declared `a -> c`; `W900` asks every `c` for an incoming `Owns`.
fn pair(c: &mut ContributionsBuilder) {
    c.kind("a", |k| {
        k.description("a");
        k.field("owner", |f| {
            f.field_type(FieldType::Reference)
                .edge("Owns")
                .target_kind("c");
        });
        k.field("likes", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("Likes")
                .target_kind("c");
        });
    });
    c.kind("c", |k| {
        k.description("c");
    });
    for label in ["Owns", "Likes"] {
        c.edge(label, |e| {
            e.source_kind("a").target_kind("c");
        });
    }
    edge_rule(
        c,
        "W900",
        CheckKind::NoIncomingEdges,
        Some("c"),
        "Owns",
        "c '{id}' is owned by nothing",
    );
}

/// `@probe/cycles`: `m.depends_on` writes `MDep` and `n.depends_on` writes
/// `NDep`, the same field name on two kinds; `W901` follows `MDep`.
fn cycles(c: &mut ContributionsBuilder) {
    kind_writing(c, "m", "depends_on", FieldType::ReferenceList, "MDep", "m");
    kind_writing(c, "n", "depends_on", FieldType::ReferenceList, "NDep", "n");
    c.edge("MDep", |e| {
        e.source_kind("m").target_kind("m");
    });
    c.edge("NDep", |e| {
        e.source_kind("n").target_kind("n");
    });
    edge_rule(
        c,
        "W901",
        CheckKind::CycleDetection,
        None,
        "MDep",
        "{kind} '{id}' is on an MDep cycle",
    );
}

/// `@probe/sides`: `a` and `b` write `Owns`, declared `a -> c`; `e` is
/// enhanced with a field writing the undeclared `Pins`; `W900` and `W903`
/// ask every `c` for an incoming `Owns` and `Pins`.
fn sides(c: &mut ContributionsBuilder) {
    for kind in ["a", "b"] {
        kind_writing(c, kind, "owner", FieldType::Reference, "Owns", "c");
    }
    c.kind("c", |k| {
        k.description("owned");
    });
    c.kind("e", |k| {
        k.description("enhanced");
    });
    c.enhance("e", "@probe/sides", |e| {
        e.field("pins", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("Pins")
                .target_kind("c");
        });
    });
    c.edge("Owns", |e| {
        e.source_kind("a").target_kind("c");
    });
    edge_rule(
        c,
        "W900",
        CheckKind::NoIncomingEdges,
        Some("c"),
        "Owns",
        "c '{id}' is owned by nothing",
    );
    edge_rule(
        c,
        "W903",
        CheckKind::NoIncomingEdges,
        Some("c"),
        "Pins",
        "c '{id}' is pinned by nothing",
    );
}

const OWNED_BY_B: &str = "a a1 \"A one\" {}\nb b1 \"B one\" { owner c1 }\nc c1 \"C one\" {}\n";

// Holds since PR #49; guards the resolution that replaces it.
#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule over an edge type several kinds write without declaring it counts the edges of every writing kind"
)]
fn an_undeclared_edge_type_counts_the_edges_of_every_writer() {
    let diagnostics = compile("@probe/implicit", OWNED_BY_B, implicit);

    assert!(messages(&diagnostics, "W900").is_empty(), "{diagnostics:?}");
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "an edge rule does not count an edge of another edge type, whatever kinds it links"
)]
fn a_declared_edge_rule_does_not_count_an_edge_of_another_type() {
    // (i) c1 is only mentioned, never owned: a Mentions edge is not Owns.
    let source = "a a1 \"A one\" { owner c2 }\nc c1 \"C one\" {}\nc c2 \"C two\" {}\n\
                  d d1 \"D one\" { mentions [c1] }\n";
    let diagnostics = compile("@probe/declared", source, declared);
    assert_eq!(
        messages(&diagnostics, "W900"),
        ["c 'c1' is owned by nothing"],
        "{diagnostics:?}"
    );

    // (ii) the same pair of kinds, two fields: a Likes edge is not Owns.
    let source = "a a1 \"A one\" { likes [c1] }\nc c1 \"C one\" {}\n";
    let diagnostics = compile("@probe/pair", source, pair);
    assert_eq!(
        messages(&diagnostics, "W900"),
        ["c 'c1' is owned by nothing"],
        "{diagnostics:?}"
    );
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "a cycle_detection rule follows only the edges of its edge type, never another kind's field of the same name"
)]
fn a_cycle_rule_follows_only_the_edges_of_its_type() {
    // An NDep loop is not an MDep cycle, though both are `depends_on`.
    let loop_of_n = "n n1 \"N one\" { depends_on [n2] }\nn n2 \"N two\" { depends_on [n1] }\n";
    let diagnostics = compile("@probe/cycles", loop_of_n, cycles);
    assert!(messages(&diagnostics, "W901").is_empty(), "{diagnostics:?}");

    let both = format!(
        "{loop_of_n}m m1 \"M one\" {{ depends_on [m2] }}\nm m2 \"M two\" {{ depends_on [m1] }}\n"
    );
    let diagnostics = compile("@probe/cycles", &both, cycles);
    assert_eq!(
        messages(&diagnostics, "W901"),
        ["m 'm1' is on an MDep cycle", "m 'm2' is on an MDep cycle"],
        "{diagnostics:?}"
    );
}

/// `@probe/typed`: `a.owner` writes `Owns` and `d.mentions` writes
/// `Mentions` (both declared); `d.watches` is a reference field that writes
/// no edge type.
fn typed(c: &mut ContributionsBuilder) {
    kind_writing(c, "a", "owner", FieldType::Reference, "Owns", "c");
    c.kind("c", |k| {
        k.description("c");
    });
    c.kind("d", |k| {
        k.description("d");
        k.field("mentions", |f| {
            f.field_type(FieldType::ReferenceList)
                .edge("Mentions")
                .target_kind("c");
        });
        k.field("watches", |f| {
            f.field_type(FieldType::ReferenceList).target_kind("c");
        });
    });
    for label in ["Owns", "Mentions"] {
        c.edge(label, |e| {
            e.target_kind("c");
        });
    }
}

#[spec(
    behavior = "snapshot_entities_once",
    verify = "every edge in the snapshot is typed by the edge type its field writes from the kind it starts at, and counted by that type"
)]
fn the_snapshot_types_each_edge_by_the_field_that_wrote_it() {
    let source = "a a1 \"A one\" { owner c2 }\nc c1 \"C one\" {}\nc c2 \"C two\" {}\n\
                  d d1 \"D one\" { mentions [c1], watches [c1, c2] }\n";
    let project = compiled("@probe/typed", source, typed);
    let entities = project.entities();

    // Each edge is typed once, by the field that wrote it.
    let mut edges: Vec<(&str, &str, &str, Option<&str>)> = entities
        .edges()
        .iter()
        .map(|e| {
            (
                e.source.as_str(),
                e.target.as_str(),
                e.label.as_str(),
                e.edge_type.as_deref(),
            )
        })
        .collect();
    edges.sort();
    assert_eq!(
        edges,
        [
            ("a1", "c2", "owner", Some("Owns")),
            ("d1", "c1", "mentions", Some("Mentions")),
            ("d1", "c1", "watches", None),
            ("d1", "c2", "watches", None),
        ]
    );

    // And counted by that type: an edge of no type counts in the total only.
    use specforge_registry::entity::Direction::{Incoming, Outgoing};
    let (c1, _) = entities.get("c1").unwrap();
    assert_eq!(c1.edges(Incoming, None), 2);
    assert_eq!(c1.edges(Incoming, Some("Mentions")), 1);
    assert_eq!(c1.edges(Incoming, Some("Owns")), 0);
    let (c2, _) = entities.get("c2").unwrap();
    assert_eq!(c2.edges(Incoming, None), 2);
    assert_eq!(c2.edges(Incoming, Some("Owns")), 1);
    let (d1, _) = entities.get("d1").unwrap();
    assert_eq!(d1.edges(Outgoing, None), 3);
    assert_eq!(d1.edges(Outgoing, Some("Mentions")), 1);
}

#[spec(
    behavior = "registry_build_edges",
    verify = "an edge type names every field that writes it: a kind's own, shared and enhancement fields alike"
)]
fn an_enhancement_field_writes_its_edge_type() {
    let source = "b b1 \"B one\" { owner c1 }\nc c1 \"C one\" {}\nc c2 \"C two\" {}\n\
                  e e1 \"E one\" { pins [c2] }\n";
    let diagnostics = compile("@probe/sides", source, sides);

    assert_eq!(
        messages(&diagnostics, "W903"),
        ["c 'c1' is pinned by nothing"],
        "{diagnostics:?}"
    );
    assert_eq!(
        messages(&diagnostics, "W021")
            .into_iter()
            .filter(|m| m.contains("field 'pins' on kind 'e'"))
            .count(),
        1,
        "{diagnostics:?}"
    );
}

#[spec(
    behavior = "registry_build_edges",
    verify = "a field that writes a declared edge type from a kind outside its declared source kind, or to a kind outside its declared target kind, is W021 and writes no edge of that type"
)]
fn a_field_outside_a_declared_end_does_not_write_the_edge_type() {
    let source = "b b1 \"B one\" { owner c1 }\nc c1 \"C one\" {}\nc c2 \"C two\" {}\n";
    let diagnostics = compile("@probe/sides", source, sides);

    assert_eq!(
        messages(&diagnostics, "W021")
            .into_iter()
            .filter(|m| m.contains("field 'owner'"))
            .collect::<Vec<_>>(),
        [
            "extension '@probe/sides': field 'owner' on kind 'b' writes edge type 'Owns', \
             whose declared source_kind is 'a' — its edges are not of that type"
        ],
        "{diagnostics:?}"
    );
    // The declared end is the contract: b1's edge is not an Owns edge.
    assert_eq!(
        messages(&diagnostics, "W900"),
        ["c 'c1' is owned by nothing", "c 'c2' is owned by nothing"],
        "{diagnostics:?}"
    );
}

// REGRESSION: the builtins' edge rules keep their meaning on a fixture project.
// This repository's own spec is the check list's `specs` check: no errors, and
// warnings at gate/baseline.json; its info counts are pinned nowhere. Never flipped.
#[test]
fn the_builtins_edge_rules_keep_their_meaning() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/todo-app");
    let runtime = specforge_component::ComponentRuntime::new();
    let names: Vec<String> = [
        "@specforge/software",
        "@specforge/testing",
        "@specforge/cargo-test",
        "@specforge/product",
        "@specforge/governance",
        "@specforge/formal",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect();
    specforge_component::builtins::load_builtins_for(&runtime, &names).unwrap();
    let diagnostics = TestProject::copy_of(root)
        .compile_in(Arc::new(runtime))
        .diagnostics();

    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["I080", "I080", "I080"], "{diagnostics:?}");
}
