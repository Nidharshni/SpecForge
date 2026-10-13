//! `GraphBuild`: a cold build is every file applied at once, and any
//! sequence of applies leaves the build of the same files (ADR 0032).

use std::collections::BTreeMap;

use specforge_graph::{FileChange, Graph, GraphBuild, GraphConfig};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

fn file(path: &str, text: &str) -> specforge_graph::SpecFile {
    parse(text, path)
}

fn parsed(path: &str, text: &str) -> FileChange {
    FileChange::Parsed(file(path, text))
}

/// Every node (id, kind, file, title, fields) and edge of a graph.
fn graph_contents(graph: &Graph) -> (Vec<String>, Vec<String>) {
    let mut nodes: Vec<String> = graph
        .nodes()
        .iter()
        .map(|n| {
            format!(
                "{} {} {}:{} {:?} {}",
                n.id.raw,
                n.kind.raw,
                n.source_span.file,
                n.source_span.start_line,
                n.title,
                serde_json::to_string(&specforge_graph::field_form::write_fields(&n.fields))
                    .unwrap()
            )
        })
        .collect();
    nodes.sort();
    let mut edges: Vec<String> = graph
        .edges()
        .iter()
        .map(|e| format!("{} -{}-> {}", e.source, e.label, e.target))
        .collect();
    edges.sort();
    (nodes, edges)
}

fn diagnostics_of(build: &GraphBuild) -> Vec<String> {
    build
        .diagnostics()
        .iter()
        .map(|d| serde_json::to_string(d).unwrap())
        .collect()
}

/// The build agrees with the cold build of `files`.
fn assert_is_the_build_of(build: &GraphBuild, files: &BTreeMap<&str, &str>) {
    let cold = GraphBuild::of(
        files.iter().map(|(path, text)| file(path, text)),
        GraphConfig::default(),
    );
    assert_eq!(graph_contents(build.graph()), graph_contents(cold.graph()));
    assert_eq!(diagnostics_of(build), diagnostics_of(&cold));
    assert_eq!(
        build.files().map(|(path, _)| path).collect::<Vec<_>>(),
        files.keys().copied().collect::<Vec<_>>()
    );
}

const ALPHA: &str = "behavior alpha \"A\" { contract \"a\" }\n";
const BETA_ON_ALPHA: &str = "behavior beta \"B\" { invariants [alpha] }\n";

#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "incremental rebuild equals cold rebuild"
)]
fn applying_every_file_at_once_is_the_cold_build() {
    let files = [
        ("a.spec", ALPHA),
        ("b.spec", BETA_ON_ALPHA),
        ("c.spec", "behavior alpha \"Dup\" { contract \"dup\" }\n"),
        (
            "d.spec",
            "invariant alpha \"Other\" {\n}\ndefine thing {\n}\n",
        ),
    ];
    let mut applied = GraphBuild::new(GraphConfig::default());
    applied.apply(files.iter().map(|(path, text)| parsed(path, text)));
    let cold = GraphBuild::of(
        files.iter().map(|(path, text)| file(path, text)),
        GraphConfig::default(),
    );

    assert_eq!(
        graph_contents(applied.graph()),
        graph_contents(cold.graph())
    );
    assert_eq!(diagnostics_of(&applied), diagnostics_of(&cold));
    assert!(!cold.diagnostics().is_empty(), "the fixture reports things");
}

#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "incremental rebuild equals cold rebuild"
)]
fn each_apply_leaves_the_build_of_the_same_files() {
    let mut files: BTreeMap<&str, &str> = BTreeMap::new();
    let mut build = GraphBuild::new(GraphConfig::default());
    build.set_verify(true);

    // Duplicates of the same and of another kind across files, a define
    // block, an E003 that resolves when its target file is added, a file
    // deleted and added again.
    let steps: [(&str, Option<&str>); 9] = [
        ("b.spec", Some(BETA_ON_ALPHA)),
        ("a.spec", Some(ALPHA)),
        (
            "c.spec",
            Some("behavior alpha \"Dup\" { contract \"dup\" }\n"),
        ),
        (
            "d.spec",
            Some("invariant alpha \"Other\" {\n}\ndefine thing {\n}\n"),
        ),
        ("a.spec", None),
        ("a.spec", Some(ALPHA)),
        ("c.spec", Some("behavior gamma \"G\" { contract \"g\" }\n")),
        ("b.spec", None),
        (
            "e.spec",
            Some("behavior alpha \"Again\" { contract \"e\" }\n"),
        ),
    ];
    for (path, text) in steps {
        let change = match text {
            Some(text) => {
                files.insert(path, text);
                parsed(path, text)
            }
            None => {
                files.remove(path);
                FileChange::Removed(path.to_string())
            }
        };
        let applied = build.apply([change]);
        assert_eq!(applied.verification, Some(Ok(())), "after {path}");
        assert_eq!(applied.files, [path]);
        assert_is_the_build_of(&build, &files);
    }
}

#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "stale nodes are removed"
)]
fn removing_a_file_takes_its_entities_and_their_edges() {
    let mut build = GraphBuild::of(
        [file("a.spec", ALPHA), file("b.spec", BETA_ON_ALPHA)],
        GraphConfig::default(),
    );
    assert_eq!(build.graph().edge_count(), 1);

    let applied = build.apply([FileChange::Removed("a.spec".to_string())]);

    assert!(build.graph().node("alpha").is_none());
    assert_eq!(build.graph().edge_count(), 0);
    assert_eq!(applied.files, ["a.spec"]);
    let removed: Vec<&str> = applied
        .delta
        .removed_nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(removed, ["alpha"]);
    // beta lost its edge, and the reference no longer resolves.
    assert_eq!(applied.delta.removed_edges.len(), 1);
    assert_eq!(build.diagnostic_files(), ["b.spec"]);
    assert_eq!(applied.changed_diagnostic_files, ["b.spec"]);
}

#[specforge_test(behavior = "rebuild_affected_subgraph", verify = "new nodes are added")]
fn a_new_file_adds_its_entities() {
    let mut build = GraphBuild::of([file("b.spec", BETA_ON_ALPHA)], GraphConfig::default());
    assert_eq!(build.graph().node_count(), 1);
    assert_eq!(build.diagnostic_files(), ["b.spec"], "alpha is unresolved");

    let applied = build.apply([parsed("a.spec", ALPHA)]);

    assert!(build.graph().node("alpha").is_some());
    let added: Vec<&str> = applied
        .delta
        .added_nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(added, ["alpha"]);
    assert!(build.diagnostics().is_empty());
    assert_eq!(applied.changed_diagnostic_files, ["b.spec"]);
}

#[specforge_test(
    behavior = "link_entity_references",
    verify = "cross-file duplicate entity ID produces E002 naming the first declaration"
)]
fn a_duplicate_in_an_unchanged_file_takes_over_when_the_first_goes() {
    let first = "behavior alpha \"First\" { contract \"one\" }\n";
    let second = "behavior alpha \"Second\" { contract \"two\" }\n";
    let mut build = GraphBuild::of(
        [file("a.spec", first), file("b.spec", second)],
        GraphConfig::default(),
    );
    build.set_verify(true);
    let node = build.graph().node("alpha").unwrap();
    assert_eq!(node.source_span.file.as_str(), "a.spec");
    let e002 = &build.diagnostics()[0];
    assert_eq!(e002.code, "E002");
    assert_eq!(
        e002.message,
        "duplicate entity ID 'alpha' (first declared at a.spec:1:1)"
    );

    let applied = build.apply([FileChange::Removed("a.spec".to_string())]);

    let node = build.graph().node("alpha").unwrap();
    assert_eq!(node.source_span.file.as_str(), "b.spec");
    assert_eq!(node.title.as_deref(), Some("Second"));
    assert!(build.diagnostics().is_empty(), "no duplicate is left");
    assert_eq!(applied.verification, Some(Ok(())));
    let modified: Vec<&str> = applied
        .delta
        .modified_nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(modified, ["alpha"], "its content differs");
}

#[test]
fn removing_a_file_the_build_does_not_hold_changes_nothing() {
    let mut build = GraphBuild::of([file("a.spec", ALPHA)], GraphConfig::default());
    build.set_verify(true);

    let applied = build.apply([FileChange::Removed("ghost.spec".to_string())]);

    assert!(applied.files.is_empty());
    assert!(applied.delta.is_empty());
    assert!(applied.changed_diagnostic_files.is_empty());
    assert_eq!(applied.verification, Some(Ok(())));
    assert!(build.graph().node("alpha").is_some());
}

#[test]
fn a_later_file_with_the_same_path_replaces_the_earlier_one() {
    let build = GraphBuild::of(
        [
            file("a.spec", ALPHA),
            file("a.spec", "behavior omega \"O\" { contract \"o\" }\n"),
        ],
        GraphConfig::default(),
    );
    assert!(build.graph().node("alpha").is_none());
    assert!(build.graph().node("omega").is_some());
    assert_eq!(build.files().len(), 1);
}

#[test]
fn a_format_version_header_is_a_diagnostic_of_its_file() {
    let mut build = GraphBuild::new(GraphConfig::default());
    build.set_verify(true);
    let codes = |build: &GraphBuild| -> Vec<String> {
        build.diagnostics().iter().map(|d| d.code.clone()).collect()
    };

    // An older header: I007, in the file, on the header line.
    let applied = build.apply([parsed(
        "a.spec",
        &format!("// specforge-format: 0.9\n{ALPHA}"),
    )]);
    assert_eq!(applied.verification, Some(Ok(())));
    assert_eq!(codes(&build), ["I007"]);
    assert_eq!(applied.changed_diagnostic_files, ["a.spec"]);
    let diagnostic = &build.file_diagnostics("a.spec")[0];
    assert_eq!(diagnostic.span.as_ref().unwrap().start_line, 1);

    // The same file, now current: the diagnostic goes with the header.
    let applied = build.apply([parsed(
        "a.spec",
        &format!("// specforge-format: 1.0\n{ALPHA}"),
    )]);
    assert_eq!(applied.verification, Some(Ok(())));
    assert!(codes(&build).is_empty());
    assert_eq!(applied.changed_diagnostic_files, ["a.spec"]);

    // A newer one is E019; removing the file removes it.
    let applied = build.apply([parsed(
        "a.spec",
        &format!("// specforge-format: 9.0\n{ALPHA}"),
    )]);
    assert_eq!(applied.verification, Some(Ok(())));
    assert_eq!(codes(&build), ["E019"]);
    let applied = build.apply([FileChange::Removed("a.spec".to_string())]);
    assert_eq!(applied.verification, Some(Ok(())));
    assert!(codes(&build).is_empty());
    assert_eq!(applied.changed_diagnostic_files, ["a.spec"]);
}
