//! File defaults (ADR 0149) through navigation and rename: an inherited
//! reference is one token, held by every block that inherits it.

use specforge_ops::authoring::rename;
use specforge_ops::read::export::{Format, Request, export};
use specforge_ops::read::navigate::{Direction, ReferenceQuery, Role};
use specforge_ops::read::query::{QueryRequest, query};
use specforge_ops::read::trace::{Target, trace};
use specforge_ops::view::ProjectView;
use specforge_test::prelude::*;

use crate::view_support::plain;

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
    assert_eq!(occurrence.role, Role::Reference);
    assert_eq!(occurrence.target, "Repo");
}

#[specforge_test(
    behavior = "apply_file_defaults",
    verify = "rename edits an inherited reference once"
)]
fn renaming_an_inherited_reference_edits_it_once() {
    let p = compile(SOFTWARE, &[("a.spec", BLOCKS), ("port.spec", PORT)]);
    let plan = rename::plan(&p.navigator(), "Repo", "Store").unwrap();
    let on_line = plan
        .edits
        .iter()
        .filter(|e| e.file == "a.spec" && e.line == 2)
        .count();
    assert_eq!(on_line, 1, "{:?}", plan.edits);
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
        plain(trace(&view, Target::Entity("b")).unwrap())
            .to_human()
            .contains("Repo")
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
