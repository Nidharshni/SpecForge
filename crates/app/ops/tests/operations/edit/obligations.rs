use super::*;

fn tasks() -> Compiled {
    project(&[("tasks.spec", TASKS)])
}

fn add(id: &str, kind: Option<&str>, text: &str) -> Edit {
    Edit::AddVerify {
        entity_id: id.into(),
        kind: kind.map(String::from),
        text: text.into(),
    }
}

fn remove(id: &str, kind: Option<&str>, text: &str) -> Edit {
    Edit::RemoveVerify {
        entity_id: id.into(),
        kind: kind.map(String::from),
        text: text.into(),
    }
}

#[specforge_test_macros::test(
    behavior = "edit_entity_obligation",
    verify = "a verify line is added after the last one"
)]
fn a_verify_line_is_added_after_the_last() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[add("complete_task", Some("unit"), "a new check")],
    );
    assert_eq!(
        text,
        TASKS.replace(
            "  verify integration \"an event fires\"\n",
            "  verify integration \"an event fires\"\n  verify unit \"a new check\"\n"
        )
    );
}

#[test]
fn a_verify_line_is_added_to_a_block_with_none() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[add("list_tasks", Some("unit"), "lists")],
    );
    assert!(
        text.contains("  \"\"\"\n  verify unit \"lists\"\n}"),
        "{text}"
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_obligation",
    verify = "a verify line is removed by kind and text"
)]
fn a_verify_line_is_removed_by_kind_and_text() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[remove(
            "complete_task",
            Some("unit"),
            "completing sets the status",
        )],
    );
    assert_eq!(
        text,
        TASKS.replace("  verify unit \"completing sets the status\"\n", "")
    );
    assert!(
        plan(
            &c,
            &[remove(
                "complete_task",
                Some("integration"),
                "completing sets the status"
            )]
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn an_equal_verify_line_is_not_added_twice() {
    let c = tasks();
    assert!(
        plan(
            &c,
            &[add(
                "complete_task",
                Some("unit"),
                "completing sets the status"
            )]
        )
        .unwrap()
        .is_empty()
    );
}
