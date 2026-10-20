//! A plan answers the text edits that apply it (`EditPlan::text_edits`).

use super::*;
use specforge_ops::read::navigate::apply_edits;

fn create(id: &str, file: &str) -> Edit {
    Edit::Create {
        kind: "invariant".to_string(),
        id: id.to_string(),
        title: id.to_string(),
        file: file.to_string(),
        fields: [("guarantee".to_string(), one("g"))].into(),
        verify: Vec::new(),
    }
}

#[test]
fn a_plans_edits_turn_each_planned_on_text_into_its_after_text() {
    let c = project(&[("tasks.spec", TASKS)]);
    let planned = plan(
        &c,
        &[
            Edit::AddVerify {
                entity_id: "list_tasks".to_string(),
                kind: None,
                text: "lists — TODO".to_string(),
            },
            create("new_rule", "tasks.spec"),
            set("list_tasks", "category", one("command")),
            Edit::Retitle {
                entity_id: "complete_task".to_string(),
                title: "Finish a Task".to_string(),
            },
        ],
    )
    .unwrap();
    let after = planned.after();
    let edits = planned.text_edits();
    assert!(edits.iter().all(|e| e.span.file == "tasks.spec"));
    let text = c.project.source_text("tasks.spec").unwrap();
    assert_eq!(
        apply_edits(&text, &edits).as_deref(),
        Some(after["tasks.spec"].as_str())
    );
}

#[test]
fn a_new_files_edits_are_one_insertion_at_its_start() {
    let c = project(&[("tasks.spec", TASKS)]);
    let planned = plan(&c, &[create("new_rule", "new.spec")]).unwrap();
    let after = planned.after().remove("new.spec").unwrap();
    let edits = planned.text_edits();
    assert_eq!(edits.len(), 1);
    let span = &edits[0].span;
    assert_eq!(span.file, "new.spec");
    assert_eq!(
        (span.start_line, span.start_col, span.end_line, span.end_col),
        (1, 1, 1, 1)
    );
    assert_eq!(edits[0].new_text, after);
}

#[test]
fn an_empty_plan_has_no_edits() {
    let c = project(&[("tasks.spec", TASKS)]);
    let planned = plan(&c, &[link("complete_task", "invariants", "done_stamped")]).unwrap();
    assert!(planned.is_empty());
    assert!(planned.text_edits().is_empty());
}
