use super::*;
use specforge_ops::authoring::edit::Obligation;
use std::collections::BTreeMap;

fn tasks() -> Compiled {
    project(&[("tasks.spec", TASKS)])
}

fn create(kind: &str, id: &str, file: &str, fields: &[(&str, Input)]) -> Edit {
    Edit::Create {
        kind: kind.into(),
        id: id.into(),
        title: "Archive a Task".into(),
        file: file.into(),
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
        verify: vec![Obligation {
            kind: Some("unit".into()),
            text: "archiving an unknown task fails".into(),
        }],
    }
}

fn archive(file: &str) -> Edit {
    create(
        "behavior",
        "archive_task",
        file,
        &[
            (
                "contract",
                one(
                    "When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST fail.",
                ),
            ),
            ("types", many(&["Task"])),
            ("category", one("command")),
            ("invariants", one("task_unique,done_stamped")),
        ],
    )
}

const ARCHIVE: &str = r#"behavior archive_task "Archive a Task" {
  invariants [task_unique, done_stamped]
  types      [Task]
  category   command
  contract   """
    When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST fail.
  """
  verify unit "archiving an unknown task fails"
}
"#;

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new writes the block in the canonical form after the last block of its kind"
)]
fn new_goes_after_the_last_block_of_its_kind() {
    let c = tasks();
    let text = after(&c, "tasks.spec", &[archive("tasks.spec")]);
    assert_eq!(text, format!("{TASKS}\n{ARCHIVE}"));
    assert!(is_canonical(&c, &text));
    let middle = project(&[(
        "m.spec",
        "behavior first_one \"First\" {\n  category query\n}\n\ninvariant task_unique \"Rule\" {\n  guarantee \"g\"\n}\n\ninvariant done_stamped \"Rule\" {\n  guarantee \"g\"\n}\n\ntype Task \"Task\" {\n  id string\n}\n",
    )]);
    let text = after(&middle, "m.spec", &[archive("m.spec")]);
    assert!(
        text.contains(
            "behavior first_one \"First\" {\n  category query\n}\n\nbehavior archive_task"
        ),
        "{text}"
    );
    assert!(text.contains("}\n\ninvariant task_unique"), "{text}");
}

#[test]
fn new_goes_at_the_end_without_a_sibling() {
    let c = project(&[(
        "e.spec",
        "invariant task_unique \"Rule\" {\n  guarantee \"g\"\n}\n\ninvariant done_stamped \"Rule\" {\n  guarantee \"g\"\n}\n\ntype Task \"Task\" {\n  id string\n}\n",
    )]);
    let text = after(&c, "e.spec", &[archive("e.spec")]);
    assert_eq!(
        text,
        format!(
            "invariant task_unique \"Rule\" {{\n  guarantee \"g\"\n}}\n\ninvariant done_stamped \"Rule\" {{\n  guarantee \"g\"\n}}\n\ntype Task \"Task\" {{\n  id string\n}}\n\n{ARCHIVE}"
        )
    );
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new creates a missing source file"
)]
fn new_creates_a_missing_source_file() {
    let c = tasks();
    let planned = plan(&c, &[archive("behaviors/archive.spec")]).unwrap();
    assert_eq!(
        planned.report().affected_files,
        vec!["behaviors/archive.spec"]
    );
    assert_eq!(planned.after()["behaviors/archive.spec"], ARCHIVE);
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new refuses a file that is not a project source"
)]
fn new_refuses_a_file_that_is_not_a_source() {
    let c = tasks();
    for file in [
        "../x.spec",
        "notes.txt",
        "/abs/x.spec",
        "node_modules/x.spec",
    ] {
        let (code, _) = refused(&c, &[archive(file)]);
        assert_eq!(code, "not_a_source", "{file}");
    }
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new refuses an id that is taken"
)]
fn new_refuses_a_taken_id() {
    let c = tasks();
    let (code, _) = refused(&c, &[create("behavior", "list_tasks", "tasks.spec", &[])]);
    assert_eq!(code, "entity_exists");
    let twice = [archive("tasks.spec"), archive("tasks.spec")];
    assert_eq!(refused(&c, &twice).0, "entity_exists");
}

#[test]
fn new_refuses_an_invalid_id() {
    let c = tasks();
    for id in ["a", "9lives", "has space"] {
        let (code, _) = refused(&c, &[create("behavior", id, "tasks.spec", &[])]);
        assert_eq!(code, "invalid_name", "{id}");
    }
}

#[test]
fn new_refuses_an_unknown_kind_with_a_close_match() {
    let c = tasks();
    let error = plan(
        &c,
        &[create("behaviour", "archive_task", "tasks.spec", &[])],
    )
    .unwrap_err()
    .error;
    assert_eq!(error.code, "unknown_kind");
    assert_eq!(
        error.suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[test]
fn new_refuses_an_undeclared_field() {
    let c = tasks();
    let (code, _) = refused(
        &c,
        &[create(
            "behavior",
            "archive_task",
            "tasks.spec",
            &[("categroy", one("x"))],
        )],
    );
    assert_eq!(code, "unknown_field");
}

#[test]
fn new_ranks_its_fields() {
    let c = tasks();
    let a = after(&c, "tasks.spec", &[archive("tasks.spec")]);
    let reversed = create(
        "behavior",
        "archive_task",
        "tasks.spec",
        &[
            ("invariants", one("task_unique,done_stamped")),
            ("category", one("command")),
            ("types", many(&["Task"])),
            (
                "contract",
                one(
                    "When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST fail.",
                ),
            ),
        ],
    );
    assert_eq!(after(&c, "tasks.spec", &[reversed]), a);
}

#[test]
fn a_later_edit_sees_an_entity_created_before_it() {
    let c = tasks();
    let text = after(
        &c,
        "tasks.spec",
        &[
            archive("tasks.spec"),
            link("archive_task", "invariants", "extra_rule"),
            Edit::Retitle {
                entity_id: "archive_task".into(),
                title: "Archive".into(),
            },
        ],
    );
    assert!(
        text.contains("behavior archive_task \"Archive\" {"),
        "{text}"
    );
    assert!(
        text.contains("invariants [task_unique, done_stamped, extra_rule]"),
        "{text}"
    );
}

#[test]
fn new_omits_a_field_equal_to_the_default() {
    let defaulted = "defaults behavior {\n  invariants [done_stamped]\n}\n\ninvariant done_stamped \"Done\" {\n  guarantee \"g\"\n}\n\ninvariant other_rule \"Other\" {\n  guarantee \"g\"\n}\n\nbehavior list_tasks \"List\" {\n  category query\n  contract \"The system MUST list\"\n}\n";
    let c = project(&[("d.spec", defaulted)]);
    let same = create(
        "behavior",
        "archive_task",
        "d.spec",
        &[
            ("invariants", many(&["done_stamped"])),
            ("contract", one("The system MUST archive")),
        ],
    );
    let text = after(&c, "d.spec", &[same]);
    assert_eq!(text.matches("invariants").count(), 1, "{text}");
    let other = create(
        "behavior",
        "archive_task",
        "d.spec",
        &[
            ("invariants", many(&["other_rule"])),
            ("contract", one("The system MUST archive")),
        ],
    );
    let text = after(&c, "d.spec", &[other]);
    assert!(text.contains("invariants [other_rule]"), "{text}");
}
