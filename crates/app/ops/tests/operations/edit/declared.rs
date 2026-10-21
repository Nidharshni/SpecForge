//! A typed edit's graph change is what its edits declare (ADR 0198): no verb over a real project is refused as
//! undeclared_change.

use super::*;
use specforge_project::testing::TestProject;

fn retitle(id: &str, title: &str) -> Edit {
    Edit::Retitle {
        entity_id: id.to_string(),
        title: title.to_string(),
    }
}

fn verify(id: &str, text: &str) -> (Edit, Edit) {
    let (entity_id, kind, text) = (id.to_string(), Some("unit".to_string()), text.to_string());
    (
        Edit::AddVerify {
            entity_id: entity_id.clone(),
            kind: kind.clone(),
            text: text.clone(),
        },
        Edit::RemoveVerify {
            entity_id,
            kind,
            text,
        },
    )
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "every verb's graph change is what it declares"
)]
fn every_verb_over_the_tasks_project_is_accepted() {
    let c = project(&[("tasks.spec", TASKS)]);
    let (_, remove) = verify("complete_task", "completing sets the status");
    let (add, _) = verify("complete_task", "completing stamps the time");
    let verbs = [
        set("list_tasks", "category", one("command")),
        unset("complete_task", "invariants"),
        link("list_tasks", "invariants", "extra_rule"),
        unlink("complete_task", "invariants", "done_stamped"),
        retitle("complete_task", "Complete It"),
        add,
        remove,
        create(
            "behavior",
            "archive_task",
            "tasks.spec",
            &[
                ("types", many(&["Task"])),
                ("contract", one("When asked, the system MUST archive.")),
                ("category", one("command")),
            ],
        ),
        Edit::Delete {
            entity_id: "extra_rule".into(),
            unlink_references: true,
        },
        Edit::Delete {
            entity_id: "done_stamped".into(),
            unlink_references: true,
        },
    ];
    for verb in &verbs {
        let planned = plan(&c, std::slice::from_ref(verb))
            .unwrap_or_else(|e| panic!("{verb:?}: {}", e.error.message));
        assert!(!planned.change().is_empty(), "{verb:?} changes something");
    }
    // The list at once: the deletes of one entity and the edit of another do not interfere.
    let list: Vec<Edit> = verbs[..9].to_vec();
    plan(&c, &list).unwrap_or_else(|e| panic!("{}", e.error.message));
}

#[test]
fn every_block_of_the_todo_app_is_retitled_and_given_a_verify_line_without_a_stray_change() {
    let todo = TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"));
    let compiled = todo.compile();
    let view = ProjectView::of(&compiled);
    for node in compiled.graph().nodes() {
        let id = node.id.raw.to_string();
        if node.kind.raw == "spec" {
            continue;
        }
        let title = format!("{} again", node.title.clone().unwrap_or_default());
        let (add, _) = verify(&id, "the obligation holds");
        for edit in [retitle(&id, &title), add] {
            if let Err(refusal) = edit::plan(&view, std::slice::from_ref(&edit)) {
                assert_ne!(
                    refusal.error.code.to_string(),
                    "undeclared_change",
                    "{edit:?}: {}",
                    refusal.error.message
                );
            }
        }
    }
}
