//! A block places its own values (ADR 0175 D6, D7): a verb names the field and the input, the block works out
//! the column and the line.

use super::*;

const FIELDS: &[(&str, &str)] = &[
    (
        "contract",
        "When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST fail.",
    ),
    ("category", "command"),
];

fn obligation() -> Obligation {
    Obligation {
        kind: Some("unit".into()),
        text: "archiving an unknown task fails".into(),
    }
}

/// An empty create of `kind` `id`, with no field and no obligation.
fn bare(kind: &str, id: &str) -> Edit {
    Edit::Create {
        kind: kind.into(),
        id: id.into(),
        title: "Archive a Task".into(),
        file: "tasks.spec".into(),
        fields: BTreeMap::new(),
        verify: vec![],
    }
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new writes each field as set writes it"
)]
fn new_writes_each_field_as_set_writes_it() {
    for toml in [
        "",
        "indent_width = 4\n",
        "use_tabs = true\n",
        "max_width = 40\n",
    ] {
        let c = project(&[("tasks.spec", TASKS), (".specforgefmt.toml", toml)]);
        let fields: Vec<(&str, Input)> = vec![
            ("types", many(&["Task"])),
            ("invariants", one("task_unique,done_stamped")),
        ]
        .into_iter()
        .chain(FIELDS.iter().map(|(k, v)| (*k, one(v))))
        .collect();
        let created = after(
            &c,
            "tasks.spec",
            &[create("behavior", "archive_task", "tasks.spec", &fields)],
        );
        let mut edits = vec![bare("behavior", "archive_task")];
        for (name, input) in fields.iter().rev() {
            edits.push(set("archive_task", name, input.clone()));
        }
        edits.push(Edit::AddVerify {
            entity_id: "archive_task".into(),
            kind: Some("unit".into()),
            text: obligation().text,
        });
        assert_eq!(created, after(&c, "tasks.spec", &edits), "with {toml:?}");
    }
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new writes each field as set writes it"
)]
fn new_leaves_out_a_field_equal_to_the_default_as_set_does() {
    let file = format!("defaults behavior {{\n  invariants [done_stamped]\n}}\n\n{TASKS}");
    let c = project(&[("tasks.spec", &file)]);
    let fields: Vec<(&str, Input)> = vec![
        ("invariants", one("done_stamped")),
        ("category", one("command")),
        ("contract", one(FIELDS[0].1)),
    ];
    let created = after(
        &c,
        "tasks.spec",
        &[create("behavior", "archive_task", "tasks.spec", &fields)],
    );
    let mut edits = vec![bare("behavior", "archive_task")];
    for (name, input) in fields.iter().rev() {
        edits.push(set("archive_task", name, input.clone()));
    }
    let mut obligations = edits.clone();
    obligations.push(Edit::AddVerify {
        entity_id: "archive_task".into(),
        kind: Some("unit".into()),
        text: obligation().text,
    });
    assert_eq!(created, after(&c, "tasks.spec", &obligations));
    let block = &created[created.find("behavior archive_task").unwrap()..];
    assert!(!block.contains("invariants"), "{created}");
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new writes each field as set writes it"
)]
fn new_ranks_an_open_kinds_members_by_name_as_set_in_key_order_does() {
    let c = project(&[("tasks.spec", TASKS)]);
    let fields: Vec<(&str, Input)> = vec![
        ("zeta", one("String")),
        ("alpha", one("Integer")),
        ("mid", one("List<String>")),
    ];
    let created = after(
        &c,
        "tasks.spec",
        &[create("type", "Row", "tasks.spec", &fields)],
    );
    let mut edits = vec![bare("type", "Row")];
    let mut keyed = fields.clone();
    keyed.sort_by_key(|(k, _)| *k);
    for (name, input) in keyed {
        edits.push(set("Row", name, input));
    }
    edits.push(Edit::AddVerify {
        entity_id: "Row".into(),
        kind: Some("unit".into()),
        text: obligation().text,
    });
    assert_eq!(created, after(&c, "tasks.spec", &edits));
    let (a, m, z) = (
        created.find("alpha").unwrap(),
        created.find("mid").unwrap(),
        created.find("zeta").unwrap(),
    );
    assert!(a < m && m < z, "{created}");
}

const BOTH: &[&str] = &["@specforge/software", "@specforge/formal"];

const NONE: &str = r#"defaults behavior {
  requires {
    graph_ready "Graph built"
  }
}

behavior inherits "Inherits" {
  contract "c"
}

behavior bystander "Bystander" {
  contract "c"
}

behavior empty_requires "Empty Requires" {
  contract "c"
  requires {}
}
"#;

#[test]
#[ignore = "flipped by T7"]
fn set_of_an_entry_into_a_block_field_written_as_none_writes_it_inside() {
    let c = compile(BOTH, &[("g.spec", NONE)]);
    let rows: [(&str, Vec<Edit>); 2] = [
        (
            "empty_requires",
            vec![set(
                "empty_requires",
                "requires.disk_ready",
                one("Disk readable"),
            )],
        ),
        (
            "inherits",
            vec![
                unset("inherits", "requires"),
                set("inherits", "requires.disk_ready", one("Disk readable")),
            ],
        ),
    ];
    for (id, edits) in rows {
        let planned = plan(&c, &edits).unwrap();
        assert!(
            !planned.report().added.0.iter().any(|d| d.code == "W020"),
            "{id}: an undeclared member"
        );
        let text = planned.after().remove("g.spec").unwrap();
        assert!(
            text.contains("  requires {\n    disk_ready \"Disk readable\"\n  }\n"),
            "{id}: {text}"
        );
        let held = compile(BOTH, &[("g.spec", &text)]);
        let node = held.project.graph().node(id).unwrap();
        assert!(node.fields.get("requires").is_some(), "{id}");
    }
}
const REQUIRING_AND_BARE: &str = r#"behavior check_graph "Check the Graph" {
  category command
  requires {
    graph_ready "Graph built"
  }
  contract """
    The system MUST check.
  """
}

behavior bare "Bare" {
  contract "c"
}
"#;

#[test]
#[ignore = "flipped by T7"]
fn set_of_a_new_entry_writes_its_annotations() {
    let c = compile(BOTH, &[("g.spec", REQUIRING_AND_BARE)]);
    for id in ["check_graph", "bare"] {
        let edit = Edit::Set {
            entity_id: id.into(),
            field: "requires.net_ready".into(),
            value: one("Network up"),
            annotations: Some(vec!["since=1.2".into()]),
        };
        let text = after(&c, "g.spec", &[edit]);
        let line = text
            .lines()
            .find(|l| l.trim_start().starts_with("net_ready"))
            .unwrap_or_else(|| panic!("{id}: {text}"));
        assert!(line.ends_with("@since \"1.2\""), "{id}: {text}");
    }
}
#[test]
#[ignore = "flipped by T8"]
fn new_writes_an_equal_obligation_once() {
    let c = project(&[("tasks.spec", TASKS)]);
    let Edit::Create {
        kind,
        id,
        title,
        file,
        fields,
        ..
    } = create(
        "behavior",
        "archive_task",
        "tasks.spec",
        &[("category", one("command")), ("contract", one(FIELDS[0].1))],
    )
    else {
        unreachable!()
    };
    let edit = Edit::Create {
        kind,
        id,
        title,
        file,
        fields,
        verify: vec![obligation(), obligation()],
    };
    let text = after(&c, "tasks.spec", &[edit]);
    assert_eq!(
        text.matches("verify unit \"archiving an unknown task fails\"")
            .count(),
        1,
        "{text}"
    );
}
