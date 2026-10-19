//! Where a reference is written: a parameter's name is no reference, a signature beside a written
//! item is another place, and a quoted union member names no type (ADR 0127).

use crate::navigate::{SOFTWARE, compile, keys};
use specforge_ops::authoring::rename;
use specforge_ops::read::navigate::{Direction, ReferenceQuery};
use specforge_test::prelude::*;

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

/// Each edit's span as `(line, start_col, end_col)`.
fn columns(plan: &rename::RenamePlan) -> Vec<(usize, usize, usize)> {
    plan.edits
        .iter()
        .map(|e| (e.span.start_line, e.span.start_col, e.span.end_col))
        .collect()
}

#[specforge_test(
    behavior = "find_all_references",
    verify = "a name written in a field and in a method signature is one location each"
)]
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

#[specforge_test(
    behavior = "find_all_references",
    verify = "a parameter's name is not a reference to the type it spells"
)]
fn a_parameters_name_is_no_occurrence() {
    let p = compile(SOFTWARE, &[("store.spec", STORE)]);

    assert_eq!(
        held_by(&p, "Task", "archive"),
        ["store.spec 17:21-17:25 archive->Task types"]
    );
}

#[specforge_test(
    behavior = "rename_entity_id",
    verify = "rename edits a type named in a method signature and no parameter name"
)]
fn a_rename_edits_signature_types_and_no_parameter_name() {
    let p = compile(SOFTWARE, &[("store.spec", STORE)]);

    let plan = rename::plan(&p.navigator(), "Task", "Job").unwrap();
    let edits = columns(&plan);

    assert_eq!(
        edits,
        [(1, 6, 10), (11, 10, 14), (12, 21, 25), (17, 21, 25)]
    );
}

#[specforge_test(
    behavior = "link_derived_references",
    verify = "a quoted union member derives no edge"
)]
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
    assert_eq!(edits, [(1, 6, 10)]);
}

/// A rename edits the spans navigation answers for the entity and its declaration, in their convention.
#[test]
fn a_rename_edits_the_spans_find_references_answers() {
    let p = compile(SOFTWARE, &[("store.spec", STORE)]);
    let with_declaration = ReferenceQuery {
        include_declaration: true,
        ..INCOMING
    };
    let mut found: Vec<(usize, usize, usize)> = p
        .navigator()
        .references("Task", with_declaration)
        .unwrap()
        .iter()
        .map(|o| (o.span.start_line, o.span.start_col, o.span.end_col))
        .collect();
    found.sort_unstable();
    let mut edits = columns(&rename::plan(&p.navigator(), "Task", "Job").unwrap());
    edits.sort_unstable();
    assert_eq!(edits, found);
}
