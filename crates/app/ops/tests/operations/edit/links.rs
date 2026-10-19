use super::*;

fn tasks() -> Compiled {
    project(&[("tasks.spec", TASKS)])
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "link appends the target and keeps the items as written"
)]
fn link_appends_and_keeps_items_as_written() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[link("complete_task", "invariants", "task_unique")],
    );
    assert_eq!(
        text,
        TASKS.replace("[done_stamped]", "[done_stamped, task_unique]")
    );
    assert!(is_canonical(&c, &text));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "link of a target already there writes nothing"
)]
fn link_twice_plans_nothing() {
    let c = tasks();
    assert!(
        plan(&c, &[link("complete_task", "invariants", "done_stamped")])
            .unwrap()
            .is_empty()
    );
    let once = [link("list_tasks", "invariants", "task_unique")];
    let twice = [once[0].clone(), once[0].clone()];
    assert_eq!(
        after(&c, "tasks.spec", &once),
        after(&c, "tasks.spec", &twice)
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "unlink of the last item removes the field"
)]
fn unlink_of_the_last_item_removes_the_field() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[unlink("complete_task", "invariants", "done_stamped")],
    );
    assert!(!text.contains("invariants"), "{text}");
    assert!(is_canonical(&c, &text));
}

#[test]
fn unlink_of_a_target_not_there_plans_nothing() {
    let c = tasks();
    assert!(
        plan(&c, &[unlink("complete_task", "invariants", "task_unique")])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn link_on_a_single_reference_is_not_editable_and_unlink_removes_a_match() {
    let events = "type Other \"Other\" {\n  id string\n}\n\ntype Task \"Task\" {\n  id string\n}\n\nevent task_done \"Task Done\" {\n  payload Task\n}\n";
    let c = project(&[("events.spec", events)]);
    let (code, message) = refused(&c, &[link("task_done", "payload", "Other")]);
    assert_eq!(code, "not_editable", "{message}");
    assert!(message.contains("use set"), "{message}");
    let text = after(
        &c,
        "events.spec",
        &[set("task_done", "payload", one("Other"))],
    );
    assert_eq!(text, events.replace("payload Task", "payload Other"));
    let text = after(&c, "events.spec", &[unlink("task_done", "payload", "Task")]);
    assert!(!text.contains("payload"), "{text}");
    assert!(
        plan(&c, &[unlink("task_done", "payload", "Other")])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_list_holding_a_comment_is_not_editable() {
    let commented = TASKS.replace(
        "invariants [done_stamped]",
        "invariants [\n    done_stamped, // stamps\n  ]",
    );
    let c = project(&[("tasks.spec", &commented)]);
    let (code, _) = refused(&c, &[link("complete_task", "invariants", "task_unique")]);
    assert_eq!(code, "not_editable");
}

#[test]
fn a_long_list_is_rewrapped_by_the_formatter() {
    let names: Vec<String> = (0..12)
        .map(|n| format!("some_long_invariant_name_{n}"))
        .collect();
    let rules: String = names
        .iter()
        .map(|n| format!("invariant {n} \"Rule\" {{\n  guarantee \"g\"\n}}\n\n"))
        .collect();
    let c = project(&[("tasks.spec", &format!("{rules}{TASKS}"))]);
    let edits: Vec<Edit> = names
        .iter()
        .map(|n| link("list_tasks", "invariants", n))
        .collect();
    let text = after(&c, "tasks.spec", &edits);
    assert!(text.contains("invariants ["), "{text}");
    assert!(text.lines().all(|l| l.len() <= 100), "{text}");
    assert!(is_canonical(&c, &text));
}

const ONE_VALUE: &str = "invariant inv_a \"A\" {\n  guarantee \"x\"\n}\n\ninvariant inv_b \"B\" {\n  guarantee \"y\"\n}\n\nbehavior b4 \"B4\" {\n  category   c\n  invariants inv_a\n}\n";

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "link and unlink read a list written as one value as its one item"
)]
fn unlink_reads_a_list_written_as_one_value() {
    let c = project(&[("a.spec", ONE_VALUE)]);
    let text = after(&c, "a.spec", &[unlink("b4", "invariants", "inv_a")]);
    assert!(!text.contains("invariants"), "{text}");
}

#[test]
fn link_appends_to_a_list_written_as_one_value() {
    let c = project(&[("a.spec", ONE_VALUE)]);
    let text = after(&c, "a.spec", &[link("b4", "invariants", "inv_b")]);
    assert!(text.contains("invariants [inv_a, inv_b]"), "{text}");
    assert!(is_canonical(&c, &text));
}
