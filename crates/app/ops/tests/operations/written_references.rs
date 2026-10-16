//! Where a reference is written: a parameter's name is no reference, a signature beside a written
//! item is another place, and a quoted union member names no type (ADR 0127).

use crate::navigate::{SOFTWARE, compile, keys};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::{Direction, ReferenceQuery};

const INCOMING: ReferenceQuery = ReferenceQuery {
    direction: Direction::Incoming,
    include_declaration: false,
};

const STORE: &str = "type Task \"Task\" {\n  title string\n}\n\ntype Other \"Other\" {\n  note string\n}\n\n\
port store \"Store\" {\n  direction outbound\n  types [Task]\n  method save(task: Task) -> Other\n}\n\n\
port archive \"Archive\" {\n  direction outbound\n  method keep(Task: Task) -> Other\n}\n";

const LEVEL: &str = "type Task \"Task\" {\n  title string\n}\n\ntype Other \"Other\" {\n  note string\n}\n\n\
type Level = \"Task\" | Other\n";

/// The incoming references of `target` held by `holder`.
fn held_by(p: &crate::navigate::Compiled, target: &str, holder: &str) -> Vec<String> {
    let found = p.navigator().references(target, INCOMING).unwrap();
    keys(&found)
        .into_iter()
        .filter(|key| key.contains(&format!(" {holder}->")))
        .collect()
}

/// Each edit as `(line, start_col, end_col)`.
fn columns(plan: &rename::RenamePlan) -> Vec<(usize, usize, usize)> {
    plan.edits
        .iter()
        .map(|e| (e.line, e.start_col, e.end_col))
        .collect()
}

#[test]
#[ignore = "flipped by T7"]
fn a_name_written_in_the_field_and_in_a_signature_is_one_occurrence_each() {
    let p = compile(SOFTWARE, &[("store.spec", STORE)]);

    assert_eq!(
        held_by(&p, "Task", "store"),
        [
            "store.spec 11:10-11:14 store->Task types",
            "store.spec 12:21-12:25 store->Task types"
        ]
    );
}

#[test]
#[ignore = "flipped by T7"]
fn a_parameters_name_is_no_occurrence() {
    let p = compile(SOFTWARE, &[("store.spec", STORE)]);

    assert_eq!(
        held_by(&p, "Task", "archive"),
        ["store.spec 17:21-17:25 archive->Task types"]
    );
}

#[test]
#[ignore = "flipped by T7"]
fn a_rename_edits_signature_types_and_no_parameter_name() {
    let p = compile(SOFTWARE, &[("store.spec", STORE)]);

    let plan = rename::plan(&p.navigator(), "Task", "Job").unwrap();
    let edits = columns(&plan);

    assert_eq!(edits, [(1, 5, 9), (11, 9, 13), (12, 20, 24), (17, 20, 24)]);
}

#[test]
#[ignore = "flipped by T4"]
fn a_quoted_union_variant_is_no_reference() {
    let p = compile(SOFTWARE, &[("level.spec", LEVEL)]);

    assert!(
        p.navigator()
            .references("Task", INCOMING)
            .unwrap()
            .is_empty()
    );
    let plan = rename::plan(&p.navigator(), "Task", "Job").unwrap();
    let edits = columns(&plan);
    assert_eq!(edits, [(1, 5, 9)]);
}
