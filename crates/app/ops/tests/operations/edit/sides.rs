//! A typed edit naming the other side of a relation (ADR 0150, ADR 0157).

use super::*;

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "link on the other side of a relation names the owning field of the target's kind"
)]
fn link_on_the_other_side_names_the_owning_field() {
    let c = project(&[("tasks.spec", TASKS)]);
    let error = plan(&c, &[link("task_unique", "enforced_by", "complete_task")])
        .unwrap_err()
        .error;
    assert_eq!(error.code, "other_side", "{}", error.message);
    assert_eq!(
        error.message,
        "field 'enforced_by' on invariant 'task_unique' is the other side of behavior.invariants: a relation is written on its owning side only"
    );
    assert_eq!(
        error.suggestion.as_deref(),
        Some("change it on the owning side: 'invariants' of behavior 'complete_task'")
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "set refuses the other side of a relation naming its owning field"
)]
fn set_on_the_other_side_names_the_owning_field() {
    let c = project(&[("tasks.spec", TASKS)]);
    let error = plan(
        &c,
        &[set("task_unique", "enforced_by", many(&["complete_task"]))],
    )
    .unwrap_err()
    .error;
    assert_eq!(error.code, "other_side", "{}", error.message);
    assert_eq!(
        error.suggestion.as_deref(),
        Some("change it on the owning side: 'invariants' of each behavior it names")
    );
}

#[test]
fn a_link_target_tells_the_other_side_by_its_kind() {
    let c = project(&[
        ("tasks.spec", TASKS),
        ("events.spec", "event task_done \"Task Done\" {\n}\n"),
    ]);
    let error = plan(&c, &[link("task_done", "trigger", "complete_task")])
        .unwrap_err()
        .error;
    assert_eq!(error.code, "other_side", "{}", error.message);
    assert!(
        error
            .message
            .contains("the other side of behavior.consumes or behavior.produces:"),
        "{}",
        error.message
    );
    assert_eq!(
        error.suggestion.as_deref(),
        Some("change it on the owning side: 'consumes' or 'produces' of behavior 'complete_task'")
    );
}

#[test]
fn an_unknown_field_with_no_owner_keeps_its_near_miss() {
    let c = project(&[("tasks.spec", TASKS)]);
    let error = plan(&c, &[link("complete_task", "invariant", "task_unique")])
        .unwrap_err()
        .error;
    assert_eq!(error.code, "unknown_field", "{}", error.message);
    assert_eq!(
        error.suggestion.as_deref(),
        Some("did you mean 'invariants'?")
    );
}
