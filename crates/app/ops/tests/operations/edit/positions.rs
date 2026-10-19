//! A plan answers the position edits that apply it (`EditPlan::edits`).

use super::*;

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
    let edits = planned.edits();
    assert_eq!(edits.len(), 1);
    for (key, changes) in edits {
        let mut text = c.project.source_text(key).unwrap().to_string();
        let mut end = text.len() + 1;
        for change in changes.iter().rev() {
            assert!(change.range.end < end, "sorted, apart: {changes:?}");
            assert!(
                text[change.range.clone()] != change.new_text,
                "a no-op: {change:?}"
            );
            end = change.range.start;
            text.replace_range(change.range.clone(), &change.new_text);
        }
        assert_eq!(text, after[key]);
    }
}

#[test]
fn a_new_files_edits_are_one_insertion_at_its_start() {
    let c = project(&[("tasks.spec", TASKS)]);
    let planned = plan(&c, &[create("new_rule", "new.spec")]).unwrap();
    let after = planned.after().remove("new.spec").unwrap();
    let edits = planned.edits();
    assert_eq!(edits.len(), 1);
    let (key, changes) = &edits[0];
    assert_eq!(*key, "new.spec");
    assert_eq!(
        changes,
        &[specforge_ops::authoring::format::Edit {
            range: 0..0,
            new_text: after
        }]
    );
}

#[test]
fn an_empty_plan_has_no_edits() {
    let c = project(&[("tasks.spec", TASKS)]);
    let planned = plan(&c, &[link("complete_task", "invariants", "done_stamped")]).unwrap();
    assert!(planned.is_empty());
    assert!(planned.edits().is_empty());
}
