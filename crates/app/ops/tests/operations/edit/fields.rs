use super::*;

fn tasks() -> Compiled {
    project(&[("tasks.spec", TASKS)])
}

#[test]
fn the_base_project_is_canonical() {
    let c = tasks();
    assert!(is_canonical(&c, TASKS));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "set writes the value in the form its field declares"
)]
fn set_replaces_the_value_and_keeps_annotations() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "category", one("command"))],
    );
    assert_eq!(text, TASKS.replace("category query", "category command"));
    assert!(is_canonical(&c, &text));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "set of an absent field inserts it at its rank"
)]
fn set_of_an_absent_field_inserts_it_at_its_rank() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "invariants", many(&["task_unique"]))],
    );
    let expected = TASKS.replace(
        "behavior list_tasks \"List Tasks\" {\n  category query\n  types    [Task]\n  contract \"\"\"",
        "behavior list_tasks \"List Tasks\" {\n  invariants [task_unique]\n  category   query\n  types      [Task]\n  contract   \"\"\"",
    );
    assert_eq!(text, expected);
    assert!(is_canonical(&c, &text));
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "unset removes the field and the comment above it"
)]
fn unset_removes_the_field_and_the_comment_above() {
    let c = tasks();
    let text = after(&c, "tasks.spec", &[unset("complete_task", "types")]);
    assert!(!text.contains("what completing touches"));
    assert!(!text.contains("types      [Task]"));
    assert!(text.contains("contract   \"\"\""));
    assert!(is_canonical(&c, &text));
}

#[test]
fn unset_of_an_absent_field_changes_nothing() {
    let c = tasks();
    assert!(
        plan(&c, &[unset("list_tasks", "invariants")])
            .unwrap()
            .is_empty()
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "set refuses a field the kind does not declare"
)]
fn set_refuses_an_undeclared_field_with_a_close_match() {
    let c = tasks();
    let (code, message) = refused(&c, &[set("list_tasks", "categroy", one("command"))]);
    assert_eq!(code, "unknown_field");
    assert!(message.contains("categroy"), "{message}");
    let error = plan(&c, &[set("list_tasks", "categroy", one("command"))])
        .unwrap_err()
        .error;
    assert_eq!(
        error.suggestion.as_deref(),
        Some("did you mean 'category'?")
    );
}

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "retitle replaces the header title"
)]
fn retitle_replaces_the_title() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[Edit::Retitle {
            entity_id: "list_tasks".into(),
            title: "List \"All\" Tasks".into(),
        }],
    );
    assert!(
        text.contains("behavior list_tasks \"List \\\"All\\\" Tasks\" {"),
        "{text}"
    );
}

#[test]
fn an_entity_that_does_not_exist_is_refused_with_a_close_match() {
    let c = tasks();
    let error = plan(&c, &[unset("list_task", "category")]).unwrap_err();
    assert_eq!(error.error.code, "E003");
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("did you mean 'list_tasks'?")
    );
    assert_eq!(error.subject, Some(EditField::Entity));
}

#[test]
fn a_later_edit_reads_what_an_earlier_one_left() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[
            set("list_tasks", "category", one("command")),
            set("list_tasks", "category", one("query")),
        ],
    );
    assert_eq!(text, TASKS);
}

#[test]
fn only_the_touched_block_is_formatted() {
    let messy = TASKS.replace(
        "invariant task_unique \"Task Unique\" {\n  guarantee",
        "invariant task_unique \"Task Unique\" {\n        guarantee",
    );
    let c = project(&[("tasks.spec", &messy)]);
    let text = after(
        &c,
        "tasks.spec",
        &[set("list_tasks", "category", one("command"))],
    );
    assert!(
        text.contains("        guarantee \"ids are unique\""),
        "the other block is untouched"
    );
}

const REQUIRING: &str = r#"behavior check_graph "Check the Graph" {
  category command
  requires {
    graph_ready "Graph built"
    disk_ready  "Disk readable"
  }
  contract """
    The system MUST check.
  """
}
"#;

#[specforge_test_macros::test(
    behavior = "edit_entity_field",
    verify = "set of a nested entry edits one line of the block field"
)]
fn set_of_a_nested_entry_edits_one_line() {
    let c = compile(
        &["@specforge/software", "@specforge/formal"],
        &[("g.spec", REQUIRING)],
    );
    let text = after(
        &c,
        "g.spec",
        &[set(
            "check_graph",
            "requires.graph_ready",
            one("Graph compiled"),
        )],
    );
    assert_eq!(text, REQUIRING.replace("Graph built", "Graph compiled"));
    let text = after(
        &c,
        "g.spec",
        &[set("check_graph", "requires.net_ready", one("Network up"))],
    );
    assert!(
        text.contains("    net_ready   \"Network up\"\n  }"),
        "{text}"
    );
    assert!(is_canonical(&c, &text));
    let text = after(
        &c,
        "g.spec",
        &[set("check_graph", "ensures.done", one("It is done"))],
    );
    assert!(
        text.contains("ensures {\n    done \"It is done\"\n  }"),
        "{text}"
    );
}

#[test]
fn unset_of_the_last_entry_removes_the_block_field() {
    let c = compile(
        &["@specforge/software", "@specforge/formal"],
        &[("g.spec", REQUIRING)],
    );
    let text = after(
        &c,
        "g.spec",
        &[unset("check_graph", "requires.graph_ready")],
    );
    assert_eq!(text.matches("_ready").count(), 1);
    let text = after(
        &c,
        "g.spec",
        &[
            unset("check_graph", "requires.graph_ready"),
            unset("check_graph", "requires.disk_ready"),
        ],
    );
    assert!(!text.contains("requires"), "{text}");
    assert!(is_canonical(&c, &text));
}

#[test]
fn a_member_of_an_open_kind_can_be_set() {
    let source = "type Task \"Task\" {\n  id string\n}\n";
    let c = project(&[("t.spec", source)]);
    let text = after(&c, "t.spec", &[set("Task", "owner", one("Person[]"))]);
    assert_eq!(
        text,
        "type Task \"Task\" {\n  id    string\n  owner Person[]\n}\n"
    );
    let text = after(&c, "t.spec", &[set("Task", "id", one("integer"))]);
    assert_eq!(text, source.replace("string", "integer"));
    let text = after(&c, "t.spec", &[unset("Task", "id")]);
    assert_eq!(text, "type Task \"Task\" {\n}\n");
}

#[test]
fn every_member_starts_on_its_own_line_once_formatted() {
    let crowded = "behavior check_graph \"Check\" { category command contract \"\"\"\n    The system MUST check.\n  \"\"\" }\n";
    let c = project(&[("g.spec", crowded)]);
    let text = after(
        &c,
        "g.spec",
        &[set("check_graph", "category", one("query"))],
    );
    assert!(text.contains("\n  category query\n"), "{text}");
    assert!(is_canonical(&c, &text));
}

#[test]
fn a_block_with_a_kept_region_is_block_unreadable() {
    let broken = TASKS.replace("category query", "category query\n  ??? )(");
    let c = project(&[("tasks.spec", &broken)]);
    let (code, _) = refused(&c, &[set("list_tasks", "category", one("command"))]);
    assert_eq!(code, "block_unreadable");
}
