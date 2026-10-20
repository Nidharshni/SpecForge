//! Every "where is this name written" question is the graph's finder's (ADR 0176): the token at a position and
//! find-references agree with rename on a default no block takes and on a duplicate declaration.

use specforge_ops::read::navigate::ReferenceQuery;

use crate::navigate::{SOFTWARE, at, compile};

const UNTAKEN: &str = "defaults behavior {\n  ports [Repo]\n}\n\nport Repo \"Repo\" {\n}\n";
const DUPLICATE: &str = "behavior a \"A\" {\n  ports [Repo]\n}\n\nbehavior a \"Again\" {\n  ports [Repo]\n}\n\n\
                         port Repo \"Repo\" {\n  direction inbound\n}\n";

#[test]
fn the_token_at_a_default_no_block_takes_names_its_entity() {
    let p = compile(SOFTWARE, &[("main.spec", UNTAKEN)]);
    let token = p.navigator().occurrence_at("main.spec", 2, 10).unwrap();
    assert_eq!(token.target, "Repo");
    assert_eq!(at(&token.span), "main.spec 2:10-2:14");
}

#[test]
fn the_token_at_a_reference_in_a_duplicate_declaration_names_its_entity() {
    let p = compile(SOFTWARE, &[("main.spec", DUPLICATE)]);
    let token = p.navigator().occurrence_at("main.spec", 6, 10).unwrap();
    assert_eq!(token.target, "Repo");
    assert_eq!(at(&token.span), "main.spec 6:10-6:14");
}

#[test]
fn find_references_lists_a_reference_in_a_duplicate_declaration() {
    let p = compile(SOFTWARE, &[("main.spec", DUPLICATE)]);
    let found = p
        .navigator()
        .references("Repo", ReferenceQuery::default())
        .unwrap();
    let shown: Vec<String> = found
        .iter()
        .map(|o| format!("{} {}", o.holder, at(&o.span)))
        .collect();
    assert_eq!(shown, ["a main.spec 2:10-2:14", "a main.spec 6:10-6:14"]);
}

#[test]
fn the_finders_holders_are_the_graphs_edges_over_the_repositorys_spec() {
    use std::collections::BTreeSet;

    use specforge_ops::read::navigate::Direction;

    let spec = specforge_repo_root::repo_root().join("spec");
    let mut paths = Vec::new();
    collect(&spec, &mut paths);
    let texts: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(&spec)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (name, std::fs::read_to_string(path).unwrap())
        })
        .collect();
    let files: Vec<(&str, &str)> = texts
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let extensions = [
        "@specforge/product",
        "@specforge/software",
        "@specforge/testing",
        "@specforge/cargo-test",
        "@specforge/governance",
        "@specforge/formal",
    ];
    let p = compile(&extensions, &files);
    let navigator = p.navigator();
    let graph = navigator.view().graph();
    let query = ReferenceQuery {
        direction: Direction::Both,
        include_declaration: false,
    };
    let mut compared = 0;
    for node in graph.nodes() {
        let id = node.id.raw.as_str();
        let found: BTreeSet<_> = navigator
            .references(id, query)
            .unwrap()
            .iter()
            .map(|o| (o.holder, o.target, o.field.unwrap()))
            .collect();
        let edges: BTreeSet<_> = graph
            .edges_from(id)
            .into_iter()
            .chain(graph.edges_to(id))
            .map(|e| (e.source, e.target, e.label))
            .collect();
        assert_eq!(found, edges, "{id}");
        compared += edges.len();
    }
    assert!(compared > 6_000, "{compared}");
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
}
