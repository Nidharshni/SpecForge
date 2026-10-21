//! A delta over the changed files is the delta of the whole graphs (ADR 0198).

use specforge_graph::{
    GraphBuild, GraphConfig, SpecFile, compute_graph_delta, compute_graph_delta_over,
};
use specforge_parser::parse;
use specforge_test_macros::test as specforge_test;

type Files<'a> = Vec<(&'a str, &'a str)>;

fn build(files: &Files<'_>) -> GraphBuild {
    let parsed: Vec<SpecFile> = files.iter().map(|(path, text)| parse(text, path)).collect();
    GraphBuild::of(parsed, GraphConfig::default())
}

const A: &str = "behavior alpha \"A\" { contract \"a\" }\n";
const B: &str = "behavior beta \"B\" { invariants [alpha] }\n";
const C: &str = "behavior gamma \"C\" { contract \"c\" }\n";
const AC: &str =
    "behavior alpha \"A\" { contract \"a\" }\nbehavior gamma \"C\" { contract \"c\" }\n";
const CA: &str =
    "behavior gamma \"C\" { contract \"c\" }\nbehavior alpha \"A\" { contract \"a\" }\n";

#[specforge_test(
    behavior = "compute_graph_delta",
    verify = "a delta over the changed files is the delta of the whole graphs"
)]
fn a_delta_over_the_changed_files_is_the_delta_of_the_whole_graphs() {
    let cases: Vec<(Files, Files, Vec<&str>)> = vec![
        // a field value changed
        (
            vec![("a.spec", A), ("b.spec", B)],
            vec![
                ("a.spec", "behavior alpha \"A\" { contract \"other\" }\n"),
                ("b.spec", B),
            ],
            vec!["a.spec"],
        ),
        // an entity created that another file already named
        (
            vec![("a.spec", ""), ("b.spec", B)],
            vec![("a.spec", A), ("b.spec", B)],
            vec!["a.spec"],
        ),
        // an entity deleted that another file names
        (
            vec![("a.spec", A), ("b.spec", B)],
            vec![("a.spec", ""), ("b.spec", B)],
            vec!["a.spec"],
        ),
        // a title and an obligation changed
        (
            vec![("a.spec", A), ("c.spec", C)],
            vec![
                (
                    "a.spec",
                    "behavior alpha \"Alpha\" { contract \"a\" verify unit \"holds\" }\n",
                ),
                ("c.spec", C),
            ],
            vec!["a.spec"],
        ),
        // a duplicate in an unchanged file takes over when the first goes
        (
            vec![
                ("a.spec", A),
                ("c.spec", "behavior alpha \"Dup\" { contract \"d\" }\n"),
            ],
            vec![
                ("a.spec", ""),
                ("c.spec", "behavior alpha \"Dup\" { contract \"d\" }\n"),
            ],
            vec!["a.spec"],
        ),
        // a block moved within its file
        (
            vec![("a.spec", AC), ("b.spec", B)],
            vec![("a.spec", CA), ("b.spec", B)],
            vec!["a.spec"],
        ),
    ];
    for (before, after, changed) in cases {
        let (old, new) = (build(&before), build(&after));
        let delta = compute_graph_delta_over(old.graph(), new.graph(), changed.iter().copied());
        assert_eq!(
            delta,
            compute_graph_delta(old.graph(), new.graph()),
            "{after:?}"
        );
    }
}

#[test]
fn a_file_left_out_hides_its_changes() {
    let old = build(&vec![("a.spec", A)]);
    let new = build(&vec![(
        "a.spec",
        "behavior alpha \"A\" { contract \"other\" }\n",
    )]);

    let delta = compute_graph_delta_over(old.graph(), new.graph(), []);

    assert!(delta.is_empty());
    assert!(!compute_graph_delta(old.graph(), new.graph()).is_empty());
}
