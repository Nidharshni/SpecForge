use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

fn todo_app() -> TestProject {
    TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"))
}

#[specforge_test(
    behavior = "export_review_document",
    verify = "export --format markdown writes the review document of examples/todo-app and examples/shop"
)]
#[ignore = "flipped by T7"]
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
