//! File defaults (ADR 0149) through navigation and rename: an inherited
//! reference is one token, held by every block that inherits it.

use specforge_ops::authoring::rename;
use specforge_ops::read::export::{Format, Request, export};
use specforge_ops::read::navigate::{Direction, ReferenceQuery};
use specforge_ops::read::query::{QueryRequest, query};
use specforge_ops::read::trace::chain;
use specforge_ops::view::ProjectView;
use specforge_test::prelude::*;

use crate::view_support::plain;

use crate::migrate::{request, run};
use crate::navigate::{SOFTWARE, compile};

const BLOCKS: &str = "defaults behavior {\n  ports [Repo]\n}\n\n\
                      behavior a \"A\" {\n}\n\nbehavior b \"B\" {\n}\n\nbehavior c \"C\" {\n}\n";
const PORT: &str = "port Repo \"Repo\" {\n}\n";

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "an inherited reference is written at the default"
)]
fn the_occurrence_at_a_defaults_token_is_its_reference() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let occurrence = p.navigator().occurrence_at("a.spec", 2, 10).unwrap();
    assert_eq!(occurrence.target, "Repo");
    assert_eq!(crate::navigate::at(&occurrence.span), "a.spec 2:10-2:14");
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "rename edits an inherited reference once"
)]
fn renaming_an_inherited_reference_edits_it_once() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let plan = rename::plan(&p.navigator(), "Repo", "Store").unwrap();
    let on_line = plan
        .edits()
        .iter()
        .filter(|e| e.span.file == "a.spec" && e.span.start_line == 2)
        .count();
    assert_eq!(on_line, 1, "{:?}", plan.edits());
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "navigation answers an inherited reference's token once"
)]
fn tokens_hold_an_inherited_reference_once() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let query = ReferenceQuery {
        direction: Direction::Incoming,
        include_declaration: true,
    };
    let mut at: Vec<String> = p
        .navigator()
        .tokens("Repo", true)
        .unwrap()
        .iter()
        .map(|o| crate::navigate::at(&o.span))
        .collect();
    at.sort();
    assert_eq!(at, ["a.spec 2:10-2:14", "port.spec 1:6-1:10"]);
    // A holder per block stays with `references` (ADR 0149 D11).
    assert_eq!(p.navigator().references("Repo", query).unwrap().len(), 4);
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "find-references lists an inherited reference once per block"
)]
fn find_references_lists_an_inherited_reference_once_per_block() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let incoming = ReferenceQuery {
        direction: Direction::Incoming,
        include_declaration: false,
    };
    let found = p.navigator().references("Repo", incoming).unwrap();
    let mut holders: Vec<_> = found.iter().map(|o| o.holder.to_string()).collect();
    holders.sort();
    assert_eq!(holders, ["a", "b", "c"]);
    assert!(
        found
            .iter()
            .all(|o| crate::navigate::at(&o.span) == "a.spec 2:10-2:14")
    );
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "query, trace and the graph export show the inherited list"
)]
fn query_trace_and_export_show_the_inherited_list() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let view = ProjectView::of(&p.project);

    let queried = plain(
        query(
            &view,
            &QueryRequest {
                entity_id: "b",
                depth: Some(0),
                ..QueryRequest::default()
            },
        )
        .unwrap(),
    );
    let queried: serde_json::Value = serde_json::from_str(&queried).unwrap();
    assert_eq!(
        queried["nodes"][0]["fields"]["ports"],
        serde_json::json!(["Repo"])
    );

    assert!(
        plain(chain(&view, "b").unwrap())
            .chain
            .downstream
            .iter()
            .any(|link| link.entity_id == "Repo")
    );

    let exported = plain(
        export(
            &view,
            &Request {
                format: Some(Format::Graph),
                ..Request::default()
            },
        )
        .unwrap(),
    );
    let exported: serde_json::Value = serde_json::from_str(&exported).unwrap();
    let with_ports = exported["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["fields"]["ports"] == serde_json::json!(["Repo"]))
        .count();
    assert_eq!(with_ports, 3);
}

/// A project of the software and product extensions holding `text` as `a.spec`.
fn project_of(text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": ["@specforge/software", "@specforge/product"]}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("a.spec"), text).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    dir
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a migration that hoists defaults keeps the graph"
)]
fn migrate_hoists_and_keeps_the_graph() {
    let block =
        |id: &str| format!("behavior {id} \"T\" {{\n  ports [Repo]\n  features [Auth]\n}}\n");
    let text = format!(
        "feature Auth \"Auth\" {{\n}}\n\nport Repo \"Repo\" {{\n}}\n\n{}",
        ["a", "b", "c"].map(block).join("\n")
    );
    let dir = project_of(&text);
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();

    let outcome = run(
        &request(&root),
        Some(specforge_project::testing::component_runtime()),
    );

    assert!(outcome.ok() && outcome.migrated(), "{outcome:?}");
    assert!(
        outcome.structural_differences().is_empty(),
        "{:?}",
        outcome.structural_differences()
    );
    let after = std::fs::read_to_string(dir.path().join("a.spec")).unwrap();
    assert!(
        after.contains("defaults behavior {\n  ports    [Repo]\n  features [Auth]\n}"),
        "{after}"
    );
    assert!(!after.contains("behavior a \"T\" {\n  ports"), "{after}");
}

#[specforge_test(
    behavior = "hoist_file_defaults",
    verify = "a field its kind does not declare is never hoisted, and the other steps are written"
)]
fn a_migration_over_a_field_its_kind_does_not_declare_writes_its_titles() {
    let block = |n: usize| {
        format!(
            "behavior b{n} \"B{n}\" {{\n  contract \"c\"\n  ports [Repo]\n  tags [Session]\n}}\n"
        )
    };
    let text = format!(
        "type Session {{\n  token string\n}}\n\nport Repo \"Repo\" {{\n  direction inbound\n}}\n\n{}",
        (1..=5).map(block).collect::<Vec<_>>().join("\n")
    );
    let dir = project_of(&text);
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();

    let outcome = run(
        &request(&root),
        Some(specforge_project::testing::component_runtime()),
    );

    assert!(outcome.ok() && outcome.migrated(), "{outcome:?}");
    let after = std::fs::read_to_string(dir.path().join("a.spec")).unwrap();
    assert!(
        after.starts_with("defaults behavior {\n  ports [Repo]\n}\n\ntype Session \"Session\" {"),
        "{after}"
    );
    assert_eq!(after.matches("tags [Session]").count(), 5, "{after}");
    assert!(outcome.structural_differences().is_empty());
}
