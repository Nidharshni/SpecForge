use super::*;

fn delete(id: &str, unlink_references: bool) -> Edit {
    Edit::Delete {
        entity_id: id.into(),
        unlink_references,
    }
}

const TWO_FILES: [(&str, &str); 2] = [
    (
        "rules.spec",
        "// the rules\n\ninvariant task_unique \"Task Unique\" {\n  guarantee \"g\"\n}\n\n// stamping\ninvariant done_stamped \"Done Stamped\" {\n  guarantee \"h\"\n}\n",
    ),
    (
        "tasks.spec",
        "behavior complete_task \"Complete\" {\n  category   command\n  invariants [done_stamped, task_unique]\n  refines    list_tasks\n}\n\nbehavior list_tasks \"List\" {\n  category    query\n  invariants  [done_stamped]\n}\n",
    ),
];

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete removes the block and the comment above it"
)]
fn delete_removes_the_block_comment_and_one_blank_line() {
    let c = project(&TWO_FILES);
    let text = after(&c, "rules.spec", &[delete("done_stamped", true)]);
    assert_eq!(
        text,
        "// the rules\n\ninvariant task_unique \"Task Unique\" {\n  guarantee \"g\"\n}\n"
    );
    let text = after(&c, "rules.spec", &[delete("task_unique", true)]);
    assert_eq!(
        text,
        "// the rules\n\n// stamping\ninvariant done_stamped \"Done Stamped\" {\n  guarantee \"h\"\n}\n"
    );
}

#[specforge_test_macros::test(
    behavior = "delete_entity",
    verify = "delete with unlink_references removes every reference"
)]
fn delete_with_unlink_references_plans_every_referrer() {
    let c = project(&TWO_FILES);
    let planned = plan(&c, &[delete("done_stamped", true)]).unwrap();
    assert_eq!(
        planned.report().affected_files,
        vec!["rules.spec", "tasks.spec"]
    );
    assert_eq!(
        planned.report().entities,
        vec!["complete_task", "done_stamped", "list_tasks"]
    );
    let tasks = &planned.after()["tasks.spec"];
    assert!(tasks.contains("invariants [task_unique]"), "{tasks}");
    assert!(!tasks.contains("done_stamped"), "{tasks}");
    assert!(is_canonical(&c, tasks));
}

#[test]
fn delete_unsets_a_single_reference_field() {
    let events = "type Task \"Task\" {\n  id string\n}\n\nevent task_done \"Task Done\" {\n  payload Task\n}\n";
    let c = project(&[("events.spec", events)]);
    let planned = plan(&c, &[delete("Task", true)]).unwrap();
    assert_eq!(
        planned.after()["events.spec"],
        "event task_done \"Task Done\" {\n}\n"
    );
}

#[test]
fn delete_of_an_entity_that_does_not_exist_is_refused() {
    let c = project(&TWO_FILES);
    assert_eq!(refused(&c, &[delete("nope", false)]).0, "E003");
    assert_eq!(
        refused(
            &c,
            &[delete("task_unique", false), delete("task_unique", false)]
        )
        .0,
        "E003"
    );
}
