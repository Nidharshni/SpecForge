//! What a list of typed edits changes taken together: the report and the planned change answer its net change
//! (ADR 0184, amended).

use super::*;
use specforge_ops::{Writes, Wrote};

fn rule(id: &str, file: &str) -> Edit {
    Edit::Create {
        kind: "invariant".into(),
        id: id.into(),
        title: "Fresh Rule".into(),
        file: file.into(),
        fields: BTreeMap::from([("guarantee".to_string(), one("g"))]),
        verify: vec![],
    }
}

fn delete(id: &str, unlink_references: bool) -> Edit {
    Edit::Delete {
        entity_id: id.into(),
        unlink_references,
    }
}

fn retitle(id: &str, title: &str) -> Edit {
    Edit::Retitle {
        entity_id: id.into(),
        title: title.into(),
    }
}

fn tasks() -> crate::navigate::Compiled {
    project(&[("tasks.spec", TASKS)])
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "a file new creates is not written when the later edits leave it empty"
)]
fn a_new_file_the_edits_leave_empty_is_not_written() {
    let c = tasks();
    let edits = [
        rule("fresh_rule", "rules/new.spec"),
        link("list_tasks", "invariants", "fresh_rule"),
        delete("fresh_rule", true),
    ];
    let edited = edit::run(
        &ProjectView::of(&c.project),
        &edits,
        false,
        specforge_ops::testing::caller(),
    )
    .unwrap();
    assert_eq!(edited.wrote, Wrote::Ran(Writes::none()));
    assert!(edited.report.affected_files.is_empty());
    assert!(edited.report.diff.is_empty());
    assert!(!c.project.environment().spec_root().join("rules").exists());
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "the report names what the edits change taken together"
)]
fn an_edit_list_that_cancels_out_names_no_entity() {
    let c = tasks();
    let lists = [
        vec![
            link("list_tasks", "invariants", "extra_rule"),
            unlink("list_tasks", "invariants", "extra_rule"),
        ],
        vec![
            retitle("complete_task", "Other"),
            retitle("complete_task", "Complete a Task"),
        ],
        vec![
            rule("archive_rule", "tasks.spec"),
            delete("archive_rule", false),
        ],
        vec![
            rule("fresh_rule", "rules/new.spec"),
            link("list_tasks", "invariants", "fresh_rule"),
            delete("fresh_rule", true),
        ],
    ];
    for (n, edits) in lists.iter().enumerate() {
        let planned = plan(&c, edits).unwrap();
        assert!(
            planned.report().entities.is_empty(),
            "list {}: {:?}",
            n + 1,
            planned.report()
        );
        if n < 3 {
            assert_eq!(
                planned.change().affected_files().count(),
                0,
                "list {}",
                n + 1
            );
        }
    }
}

#[test]
fn a_real_change_beside_a_cancelled_one_names_only_its_entity() {
    let c = tasks();
    let planned = plan(
        &c,
        &[
            retitle("complete_task", "Other"),
            retitle("complete_task", "Complete a Task"),
            set("list_tasks", "category", one("command")),
        ],
    )
    .unwrap();
    assert_eq!(planned.report().entities, vec!["list_tasks"]);
    assert_eq!(planned.report().affected_files, vec!["tasks.spec"]);
}

#[test]
fn an_entity_moved_to_another_file_is_named() {
    let c = tasks();
    let planned = plan(
        &c,
        &[
            delete("extra_rule", true),
            Edit::Create {
                kind: "invariant".into(),
                id: "extra_rule".into(),
                title: "Extra Rule".into(),
                file: "more.spec".into(),
                fields: BTreeMap::from([("guarantee".to_string(), one("something else"))]),
                verify: vec![],
            },
        ],
    )
    .unwrap();
    assert_eq!(planned.report().entities, vec!["extra_rule"]);
    assert_eq!(
        planned.report().affected_files,
        vec!["more.spec", "tasks.spec"]
    );
}

#[test]
fn a_block_a_create_moves_down_is_not_named() {
    let c = tasks();
    let planned = plan(&c, &[rule("archive_rule", "tasks.spec")]).unwrap();
    assert_eq!(planned.report().entities, vec!["archive_rule"]);
}
