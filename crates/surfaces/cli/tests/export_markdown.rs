use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

fn todo_app() -> TestProject {
    TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"))
}

fn shop() -> TestProject {
    TestProject::copy_of(specforge_repo_root::repo_root().join("examples/shop"))
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "export --format markdown writes the review document of examples/todo-app and examples/shop"
)]
fn todo_app_review_document() {
    let run = Cli::on(&todo_app())
        .run(&["export", "--format", "markdown"])
        .exits(0);
    assert_eq!(
        run.stdout.matches("\n## ").count(),
        30,
        "one section per entity"
    );
    insta::assert_snapshot!("todo_app_review_document", run.stdout);
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "export --format markdown writes the review document of examples/todo-app and examples/shop"
)]
fn shop_review_document() {
    let run = Cli::on(&shop())
        .run(&["export", "--format", "markdown"])
        .exits(0);
    insta::assert_snapshot!("shop_review_document", run.stdout);
}

#[test]
fn markdown_ignores_the_budget_and_the_schema_flags() {
    let project = shop();
    let plain = Cli::on(&project)
        .run(&["export", "--format", "markdown"])
        .exits(0);
    let flagged = Cli::on(&project)
        .run(&[
            "export",
            "--format",
            "markdown",
            "--max-tokens",
            "5",
            "--with-schema",
        ])
        .exits(0);
    assert_eq!(flagged.stdout, plain.stdout);
}

#[test]
fn a_written_reference_that_does_not_resolve_is_in_the_document() {
    let project = shop();
    let source = project.read("spec/cart.spec");
    project.write(
        "spec/cart.spec",
        source.replace(
            "invariants [no_duplicate_items]",
            "invariants [no_duplicate_items, ghost_inv]",
        ),
    );
    let run = Cli::on(&project)
        .run(&["export", "--format", "markdown"])
        .exits(0);
    assert!(
        run.stdout
            .contains("| invariants | `ghost_inv` | — | *unresolved* |"),
        "{}",
        run.stdout
    );
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "export --format markdown writes the review document of examples/todo-app and examples/shop"
)]
fn complete_task_reads_as_a_review_document() {
    let project = todo_app();
    let run = Cli::on(&project)
        .run(&[
            "export",
            "--format",
            "markdown",
            "--scope",
            "complete_task",
            "--depth",
            "0",
        ])
        .exits(0);
    insta::assert_snapshot!(run.stdout, @r###"
    # Specification

    1 entity around `complete_task` (depth 0).

    ## behavior `complete_task` — Complete a Task

    `behaviors/task.spec:28`

    **contract**

    > When a CompleteTaskCommand is received for an existing task, the system MUST
    > set the task status to `done`, set completedAt to the current time, and emit
    > a task_completed event. An unknown task id MUST return TaskNotFoundError.
    > It MUST return Result\<Task, TaskNotFoundError>.

    **Fields**

    | Field | Value |
    |---|---|
    | category | command |

    **Links**

    | Field | Entity | Kind | Title |
    |---|---|---|---|
    | invariants | `completed_implies_timestamp` | invariant | Completed Tasks Have a Timestamp |
    | types | `Task` | type | A unit of work the user wants to track |
    | types | `CompleteTaskCommand` | type | — |
    | types | `TaskNotFoundError` | type | — |
    | ports | `TaskRepository` | port | Persistence boundary for tasks |
    | produces | `task_completed` | event | Task Completed |
    | features | `task_management` | feature | Task Management |
    | satisfies | `no_lost_completion` | property | Completion is never lost |

    **Verify**

    | # | Kind | Obligation |
    |---|---|---|
    | 1 | unit | completing an unknown task returns TaskNotFoundError |
    | 2 | unit | completing a task sets status to done |
    | 3 | integration | task_completed event fires once per completion |
    "###);
}
