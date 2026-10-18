//! The typed-edit verbs (`new`, `set`, `unset`, `link`, `unlink`, `delete`) on a copy of `examples/todo-app`: each
//! writes the canonical form, a refused edit writes nothing, and every file written is format-clean (ADR 0151).

use crate::support::{Cli, Run, TestProject};
use specforge_test_macros::test as specforge_test;

const TASKS: &str = "spec/behaviors/task.spec";

fn todo() -> TestProject {
    TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"))
}

fn sf(project: &TestProject, args: &[&str]) -> Run {
    Cli::on(project).run(args)
}

fn assert_format_clean(project: &TestProject) {
    let check = sf(project, &["format", "--check"]);
    assert_eq!(check.code, 0, "format --check\n{}", check.stdout);
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new writes the block in the canonical form after the last block of its kind"
)]
#[ignore = "flipped by T9"]
fn new_writes_the_canonical_block() {
    let project = todo();
    let before = project.read(TASKS);
    sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive a Task",
            "--in",
            "behaviors/task.spec",
            "--category",
            "command",
            "--types",
            "Task,TaskNotFoundError",
            "--ports",
            "TaskRepository",
            "--features",
            "task_management",
            "--contract",
            "When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST return TaskNotFoundError.",
            "--verify",
            "unit:archiving an unknown task returns TaskNotFoundError",
        ],
    )
    .exits(0);
    // `ports` and `features` equal the file's defaults, so no line is written for them.
    let block = r#"
behavior archive_task "Archive a Task" {
  types    [Task, TaskNotFoundError]
  category command
  contract """
    When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST return TaskNotFoundError.
  """
  verify unit "archiving an unknown task returns TaskNotFoundError"
}
"#;
    assert_eq!(project.read(TASKS), format!("{before}{block}"));
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "edit_entity_link",
    verify = "link appends the target and keeps the items as written"
)]
#[specforge_test(
    behavior = "edit_entity_field",
    verify = "unset removes the field and the comment above it"
)]
#[specforge_test(
    behavior = "edit_entity_field",
    verify = "set of an absent field inserts it at its rank"
)]
#[ignore = "flipped by T9"]
fn link_unset_and_a_new_field_are_laid_out_by_the_formatter() {
    let project = todo();
    sf(
        &project,
        &["link", "complete_task", "invariants", "task_id_uniqueness"],
    )
    .exits(0);
    let unset = sf(&project, &["unset", "complete_task", "satisfies"]).exits(0);
    assert!(unset.stdout.contains("W123"), "{}", unset.stdout);
    sf(
        &project,
        &["link", "list_tasks", "invariants", "task_id_uniqueness"],
    )
    .exits(0);
    let text = project.read(TASKS);
    assert!(
        text.contains("invariants [completed_implies_timestamp, task_id_uniqueness]"),
        "{text}"
    );
    assert!(!text.contains("satisfies"), "{text}");
    assert!(
        text.contains("invariants [task_id_uniqueness]\n  category   query"),
        "{text}"
    );
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "validate_typed_edit",
    verify = "an edit that adds an error is E080 and writes nothing"
)]
#[ignore = "flipped by T9"]
fn a_dangling_link_is_e080_and_writes_nothing() {
    let project = todo();
    let before = project.read(TASKS);
    let refused = sf(
        &project,
        &[
            "link",
            "complete_task",
            "produces",
            "task_archived",
            "--format",
            "json",
        ],
    );
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(refused.stdout.contains("E080"), "{}", refused.stdout);
    assert_eq!(project.read(TASKS), before);
}

#[specforge_test(
    behavior = "edit_entity_field",
    verify = "retitle replaces the header title"
)]
#[specforge_test(
    behavior = "edit_entity_obligation",
    verify = "a verify line is added after the last one"
)]
#[specforge_test(
    behavior = "edit_entity_obligation",
    verify = "a verify line is removed by kind and text"
)]
#[ignore = "flipped by T9"]
fn set_retitle_and_verify_change_one_block() {
    let project = todo();
    let unchanged = sf(&project, &["set", "list_tasks", "category", "query"]).exits(0);
    assert!(
        unchanged.stdout.contains("nothing to write"),
        "{}",
        unchanged.stdout
    );
    sf(&project, &["set", "list_tasks", "--title", "List Tasks"]).exits(0);
    sf(
        &project,
        &[
            "link",
            "list_tasks",
            "--verify",
            "unit:listing done tasks excludes open tasks",
        ],
    )
    .exits(0);
    sf(
        &project,
        &[
            "unlink",
            "list_tasks",
            "--verify",
            "unit:listing open tasks excludes done tasks",
        ],
    )
    .exits(0);
    let text = project.read(TASKS);
    assert!(
        text.contains(r#"behavior list_tasks "List Tasks" {"#),
        "{text}"
    );
    assert!(
        text.contains(r#"verify unit "listing done tasks excludes open tasks""#),
        "{text}"
    );
    assert!(
        !text.contains("listing open tasks excludes done tasks"),
        "{text}"
    );
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "delete_entity",
    verify = "delete of a referenced entity is E080"
)]
#[specforge_test(
    behavior = "delete_entity",
    verify = "delete with unlink_references removes every reference"
)]
#[ignore = "flipped by T9"]
fn delete_is_e080_until_references_are_unlinked() {
    let project = todo();
    let refused = sf(&project, &["delete", "create_task"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(refused.stdout.contains("E080") || refused.stderr.contains("E080"));
    assert!(project.read(TASKS).contains("behavior create_task"));
    sf(&project, &["delete", "create_task", "--unlink-references"]).exits(0);
    assert!(!project.read(TASKS).contains("behavior create_task"));
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "validate_typed_edit",
    verify = "every block an edit touches is format-clean"
)]
#[ignore = "flipped by T9"]
fn every_written_file_is_format_clean() {
    let project = todo();
    sf(&project, &["set", "list_tasks", "--title", "List Tasks"]).exits(0);
    sf(
        &project,
        &["link", "complete_task", "invariants", "task_id_uniqueness"],
    )
    .exits(0);
    assert_format_clean(&project);
}
