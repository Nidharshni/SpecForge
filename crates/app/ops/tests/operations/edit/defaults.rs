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
fn unset_of_an_own_field_over_a_default_writes_none() {
    let c = defaulted();
    let text = after(&c, "tasks.spec", &[unset("list_tasks", "invariants")]);
    assert!(text.contains("invariants []"), "{text}");
    assert_eq!(text.matches("invariants").count(), 2, "{text}");
    // The way back to the default is a set to its value.
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "invariants", many(&["done_stamped"]))],
    );
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

fn added(c: &Compiled, edits: &[Edit]) -> Vec<String> {
    plan(c, edits)
        .unwrap()
        .report()
        .added
        .0
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

fn new_block(kind: &str, id: &str, file: &str, fields: &[(&str, Input)]) -> Edit {
    Edit::Create {
        kind: kind.into(),
        id: id.into(),
        title: "New".into(),
        file: file.into(),
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        verify: Vec::new(),
    }
}

const FM: &str = r#"defaults failure_mode {
  rpn 100
}

failure_mode slow_disk "Slow Disk" {
  cause    "Old hardware"
  effect   "Slow reads"
  severity high
}
"#;

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new compares what it writes with the default, not what it was given"
)]
fn new_writes_a_string_list_item_that_holds_a_comma() {
    let c = project(&[(
        "d.spec",
        "defaults behavior {\n  refs [\"a\", \"b\"]\n}\n\nbehavior list_tasks \"List\" {\n  category query\n  contract \"The system MUST list\"\n}\n",
    )]);
    let text = after(
        &c,
        "d.spec",
        &[new_block(
            "behavior",
            "archive_task",
            "d.spec",
            &[
                ("contract", one("The system MUST archive")),
                ("refs", one("a, b")),
            ],
        )],
    );
    assert!(text.contains("[\"a, b\"]"), "{text}");
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new never writes a field that restates its default"
)]
fn new_leaves_out_an_integer_equal_to_its_default() {
    let c = compile(&["@specforge/governance"], &[("fm.spec", FM)]);
    let edit = new_block(
        "failure_mode",
        "full_disk",
        "fm.spec",
        &[
            ("cause", one("Logs grow")),
            ("effect", one("Writes fail")),
            ("severity", one("high")),
            ("rpn", one("100")),
        ],
    );
    let text = after(&c, "fm.spec", std::slice::from_ref(&edit));
    assert_eq!(text.matches("rpn").count(), 1, "{text}");
    assert!(!added(&c, &[edit]).contains(&"W161".to_string()));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "a link after new starts from the file's default"
)]
fn a_link_after_new_starts_from_the_default() {
    let c = defaulted();
    let text = after(
        &c,
        "tasks.spec",
        &[
            new_block(
                "behavior",
                "archive_task",
                "tasks.spec",
                &[
                    ("category", one("command")),
                    ("contract", one("When archived, the system MUST keep it.")),
                ],
            ),
            link("archive_task", "invariants", "task_unique"),
        ],
    );
    assert!(
        text.contains("invariants [done_stamped, task_unique]"),
        "{text}"
    );
    assert!(is_canonical(&c, &text));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_link",
    verify = "a link after unset starts from none"
)]
fn a_link_after_unset_starts_from_none() {
    let c = defaulted();
    // Unset leaves `[]`, the link writes `[task_unique]` back: the block as it was.
    let planned = plan(
        &c,
        &[
            unset("list_tasks", "invariants"),
            link("list_tasks", "invariants", "task_unique"),
        ],
    )
    .unwrap();
    assert!(planned.is_empty());
    let text = after(
        &c,
        "tasks.spec",
        &[
            unset("complete_task", "invariants"),
            link("complete_task", "invariants", "task_unique"),
        ],
    );
    assert!(text.contains("invariants [task_unique]"), "{text}");
}

#[test]
fn a_link_on_a_default_written_as_one_value_extends_it() {
    let c = project(&[(
        "d.spec",
        "defaults behavior {\n  refs \"a\"\n}\n\nbehavior list_tasks \"List\" {\n  category query\n  contract \"The system MUST list\"\n}\n",
    )]);
    let text = after(&c, "d.spec", &[link("list_tasks", "refs", "b")]);
    assert!(text.contains("[\"a\", \"b\"]"), "{text}");
}
