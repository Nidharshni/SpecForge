use super::*;

const WITH_DEFAULTS: &str = r#"defaults behavior {
  invariants [done_stamped]
}

invariant task_unique "Task Unique" {
  guarantee "ids are unique"
}

invariant done_stamped "Done Stamped" {
  guarantee "done tasks carry a time"
}

behavior complete_task "Complete a Task" {
  category command
  contract """
    When completed, the system MUST set the status.
  """
}

behavior list_tasks "List Tasks" {
  category   query
  invariants [task_unique]
  contract   """
    When asked, the system MUST list.
  """
}
"#;

fn defaulted() -> Compiled {
    project(&[("tasks.spec", WITH_DEFAULTS)])
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "link of an inherited field copies the default"
)]
fn link_of_an_inherited_field_copies_the_default() {
    let c = defaulted();
    let text = after(
        &c,
        "tasks.spec",
        &[link("complete_task", "invariants", "task_unique")],
    );
    assert!(
        text.contains("behavior complete_task \"Complete a Task\" {\n  invariants [done_stamped, task_unique]\n  category   command"),
        "{text}"
    );
    assert!(text.starts_with("defaults behavior {\n  invariants [done_stamped]\n}"));
    assert!(is_canonical(&c, &text));
}

#[test]
fn link_of_a_target_the_default_has_plans_nothing() {
    let c = defaulted();
    assert!(
        plan(&c, &[link("complete_task", "invariants", "done_stamped")])
            .unwrap()
            .is_empty()
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "an own field equal to the default is removed"
)]
fn an_own_field_equal_to_the_default_is_removed() {
    let c = defaulted();
    let text = after(
        &c,
        "tasks.spec",
        &[
            unlink("list_tasks", "invariants", "task_unique"),
            link("list_tasks", "invariants", "done_stamped"),
        ],
    );
    assert_eq!(
        text.matches("invariants").count(),
        1,
        "only the default is left: {text}"
    );
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "invariants", many(&["done_stamped"]))],
    );
    assert_eq!(text.matches("invariants").count(), 1, "{text}");
}

#[test]
fn unset_of_an_inherited_list_writes_an_empty_list() {
    let c = defaulted();
    let text = after(&c, "tasks.spec", &[unset("complete_task", "invariants")]);
    assert!(text.contains("invariants []"), "{text}");
    assert!(is_canonical(&c, &text));
    let text = after(
        &c,
        "tasks.spec",
        &[unlink("complete_task", "invariants", "done_stamped")],
    );
    assert!(text.contains("invariants []"), "{text}");
}

#[test]
fn unset_of_an_own_field_over_a_default_returns_to_the_default() {
    let c = defaulted();
    let text = after(&c, "tasks.spec", &[unset("list_tasks", "invariants")]);
    assert_eq!(text.matches("invariants").count(), 1, "{text}");
}

#[test]
fn a_default_is_never_edited() {
    let c = defaulted();
    let text = after(
        &c,
        "tasks.spec",
        &[set("complete_task", "category", one("query"))],
    );
    assert!(text.starts_with("defaults behavior {\n  invariants [done_stamped]\n}"));
}
