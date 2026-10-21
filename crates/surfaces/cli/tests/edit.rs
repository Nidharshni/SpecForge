//! The typed-edit verbs (`new`, `set`, `unset`, `link`, `unlink`, `delete`) on a copy of `examples/todo-app`: each
//! writes the canonical form, a refused edit writes nothing, and every file written is format-clean (ADR 0151).

use crate::support::{Cli, Run, TestProject};
use specforge_test_macros::test as specforge_test;

const TASKS: &str = "spec/behaviors/task.spec";

fn todo() -> TestProject {
    TestProject::copy_of(specforge_repo_root::repo_root().join("examples/todo-app"))
}

fn sf(project: &TestProject, args: &[&str]) -> Run {
    Cli::on(project).run(args)
}

fn assert_format_clean(project: &TestProject) {
    let check = sf(project, &["format", "--check"]);
    assert_eq!(check.code, 0, "format --check\n{}", check.stdout);
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new writes the block in the canonical form after the last block of its kind"
)]
fn new_writes_the_canonical_block() {
    let project = todo();
    let before = project.read(TASKS);
    sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive a Task",
            "--in",
            "behaviors/task.spec",
            "--category",
            "command",
            "--types",
            "Task,TaskNotFoundError",
            "--ports",
            "TaskRepository",
            "--features",
            "task_management",
            "--contract",
            "When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST return TaskNotFoundError.",
            "--verify",
            "unit:archiving an unknown task returns TaskNotFoundError",
        ],
    )
    .exits(0);
    // `ports` and `features` equal the file's defaults, so no line is written for them.
    let block = r#"
behavior archive_task "Archive a Task" {
  types    [Task, TaskNotFoundError]
  category command
  contract """
    When an archive request names a done task, the system MUST mark it archived; an unknown task id MUST return TaskNotFoundError.
  """
  verify unit "archiving an unknown task returns TaskNotFoundError"
}
"#;
    assert_eq!(project.read(TASKS), format!("{before}{block}"));
    assert_format_clean(&project);
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new --in reads a spelling of an existing file as that file"
)]
fn new_in_a_spelling_of_an_existing_file_adds_to_it() {
    let project = todo();
    let before = project.read(TASKS);
    sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive a Task",
            "--in",
            "./behaviors/task.spec",
            "--contract",
            "The system MUST archive a done task.",
        ],
    )
    .exits(0);
    let after = project.read(TASKS);
    assert!(after.starts_with(&before), "the file as it was comes first");
    assert!(
        after.contains("behavior archive_task \"Archive a Task\" {"),
        "{after}"
    );
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new refuses a file named from the project root, naming its spec-root spelling"
)]
fn new_in_a_project_root_spelling_is_refused_naming_its_file() {
    let project = todo();
    let before = project.read(TASKS);
    let run = sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive a Task",
            "--in",
            "spec/behaviors/task.spec",
            "--contract",
            "The system MUST archive a done task.",
        ],
    )
    .exits(1);
    assert!(run.stderr.contains("error[not_a_source]"), "{}", run.stderr);
    assert!(run.stderr.contains("behaviors/task.spec"), "{}", run.stderr);
    assert_eq!(project.read(TASKS), before);
}

#[specforge_test(
    behavior = "edit_entity_link",
    verify = "link appends the target and keeps the items as written"
)]
#[specforge_test(
    behavior = "edit_entity_field",
    verify = "unset removes the field and the comment above it"
)]
#[specforge_test(
    behavior = "edit_entity_field",
    verify = "set of an absent field inserts it at its rank"
)]
fn link_unset_and_a_new_field_are_laid_out_by_the_formatter() {
    let project = todo();
    sf(
        &project,
        &["link", "complete_task", "invariants", "task_id_uniqueness"],
    )
    .exits(0);
    let unset = sf(&project, &["unset", "complete_task", "satisfies"]).exits(0);
    assert!(unset.stderr.contains("W123"), "{}", unset.stderr);
    sf(
        &project,
        &["link", "list_tasks", "invariants", "task_id_uniqueness"],
    )
    .exits(0);
    let text = project.read(TASKS);
    assert!(
        text.contains("invariants [completed_implies_timestamp, task_id_uniqueness]"),
        "{text}"
    );
    assert!(!text.contains("satisfies"), "{text}");
    assert!(
        text.contains("invariants [task_id_uniqueness]\n  category   query"),
        "{text}"
    );
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "validate_typed_edit",
    verify = "an edit that adds an error is E080 and writes nothing"
)]
fn a_dangling_link_is_e080_and_writes_nothing() {
    let project = todo();
    let before = project.read(TASKS);
    let refused = sf(
        &project,
        &[
            "link",
            "complete_task",
            "produces",
            "task_archived",
            "--format",
            "json",
        ],
    );
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(refused.stdout.contains("E080"), "{}", refused.stdout);
    assert!(
        refused.stdout.contains("task_archived"),
        "{}",
        refused.stdout
    );
    assert_eq!(project.read(TASKS), before);
}

#[specforge_test(
    behavior = "edit_entity_field",
    verify = "retitle replaces the header title"
)]
#[specforge_test(
    behavior = "edit_entity_obligation",
    verify = "a verify line is added after the last one"
)]
#[specforge_test(
    behavior = "edit_entity_obligation",
    verify = "a verify line is removed by kind and text"
)]
fn set_retitle_and_verify_change_one_block() {
    let project = todo();
    let unchanged = sf(&project, &["set", "list_tasks", "category", "query"]).exits(0);
    assert!(
        unchanged.stdout.contains("nothing to write"),
        "{}",
        unchanged.stdout
    );
    sf(&project, &["set", "list_tasks", "--title", "List Tasks"]).exits(0);
    sf(
        &project,
        &[
            "link",
            "list_tasks",
            "--verify",
            "unit:listing done tasks excludes open tasks",
        ],
    )
    .exits(0);
    sf(
        &project,
        &[
            "unlink",
            "list_tasks",
            "--verify",
            "unit:listing open tasks excludes done tasks",
        ],
    )
    .exits(0);
    let text = project.read(TASKS);
    assert!(
        text.contains(r#"behavior list_tasks "List Tasks" {"#),
        "{text}"
    );
    assert!(
        text.contains(r#"verify unit "listing done tasks excludes open tasks""#),
        "{text}"
    );
    assert!(
        !text.contains("listing open tasks excludes done tasks"),
        "{text}"
    );
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "delete_entity",
    verify = "delete of a referenced entity is E080"
)]
#[specforge_test(
    behavior = "delete_entity",
    verify = "delete with unlink_references removes every reference"
)]
fn delete_is_e080_until_references_are_unlinked() {
    let project = todo();
    let refused = sf(&project, &["delete", "create_task"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(refused.stdout.contains("E080") || refused.stderr.contains("E080"));
    assert!(project.read(TASKS).contains("behavior create_task"));
    sf(&project, &["delete", "create_task", "--unlink-references"]).exits(0);
    assert!(!project.read(TASKS).contains("behavior create_task"));
    assert_format_clean(&project);
}

#[specforge_test(
    behavior = "validate_typed_edit",
    verify = "every block an edit touches is format-clean"
)]
fn every_written_file_is_format_clean() {
    let project = todo();
    sf(&project, &["set", "list_tasks", "--title", "List Tasks"]).exits(0);
    sf(
        &project,
        &["link", "complete_task", "invariants", "task_id_uniqueness"],
    )
    .exits(0);
    assert_format_clean(&project);
}

#[test]
fn new_lists_the_kinds_fields_in_its_help() {
    let project = todo();
    let help = sf(&project, &["new", "behavior", "--help"]).exits(0);
    assert!(help.stdout.contains("--contract"), "{}", help.stdout);
    assert!(
        help.stdout.contains("The behavioral contract"),
        "{}",
        help.stdout
    );
    assert!(help.stdout.contains("--invariants <ID,"), "{}", help.stdout);
}

#[test]
fn a_misspelt_field_flag_is_a_clap_error() {
    let project = todo();
    let before = project.read(TASKS);
    let run = sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive",
            "--in",
            "behaviors/task.spec",
            "--contrcat",
            "x",
        ],
    );
    assert_eq!(run.code, 2, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stderr.contains("--contract"),
        "did you mean: {}",
        run.stderr
    );
    assert_eq!(project.read(TASKS), before);
}

#[test]
fn new_without_in_is_refused() {
    let project = todo();
    let run = sf(&project, &["new", "behavior", "archive_task", "Archive"]);
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("--in"), "{}", run.stderr);
}

#[test]
fn dry_run_prints_the_diff_and_writes_nothing() {
    let project = todo();
    let before = project.read(TASKS);
    let run = sf(
        &project,
        &["set", "list_tasks", "--title", "List Tasks", "--dry-run"],
    )
    .exits(0);
    assert!(
        run.stdout.contains("+behavior list_tasks \"List Tasks\" {"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("dry run"), "{}", run.stdout);
    assert_eq!(project.read(TASKS), before);
}

#[test]
fn json_output_is_the_edit_report() {
    let project = todo();
    let document =
        Cli::on(&project).document(&["link", "complete_task", "invariants", "task_id_uniqueness"]);
    assert_eq!(document.code, 0);
    assert_eq!(
        document.json["entities"],
        serde_json::json!(["complete_task"])
    );
    assert_eq!(
        document.json["affected_files"],
        serde_json::json!(["behaviors/task.spec"])
    );
    assert_eq!(
        document.json["files_written"],
        serde_json::json!(["spec/behaviors/task.spec"])
    );
    assert!(
        document.json["diff"]
            .as_str()
            .unwrap()
            .contains("+  invariants")
    );
    let preview = Cli::on(&project).document(&[
        "unlink",
        "complete_task",
        "invariants",
        "task_id_uniqueness",
        "--dry-run",
    ]);
    assert_eq!(preview.json["dry_run"], true);
    assert!(preview.json.get("files_written").is_none());
}

#[test]
fn no_builtin_field_shadows_a_new_option() {
    let project = todo();
    let schema = sf(&project, &["schema", "--no-edges"]).exits(0).json();
    for kind in schema["entity_kinds"].as_array().unwrap() {
        for field in kind["fields"].as_array().unwrap() {
            let name = field["name"].as_str().unwrap();
            assert!(
                ![
                    "in", "verify", "path", "format", "dry-run", "dry_run", "help", "title"
                ]
                .contains(&name),
                "{} declares '{name}'",
                kind["name"]
            );
        }
    }
}

#[test]
fn link_on_the_other_side_is_refused_naming_the_owning_field() {
    let project = todo();
    let file = "spec/invariants/task.spec";
    let before = project.read(file);
    let document =
        Cli::on(&project).document(&["link", "task_id_uniqueness", "enforced_by", "complete_task"]);
    assert_eq!(document.code, 1, "{}", document.json);
    assert_eq!(document.json["code"], "other_side", "{}", document.json);
    assert_eq!(project.read(file), before);
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new reads its field flags from the project its --path names, wherever --path stands"
)]
fn new_with_path_after_the_kind_reads_that_projects_fields() {
    let project = todo();
    let outside = tempfile::TempDir::new().unwrap();
    let run = Cli::on(&project).in_dir(outside.path()).run(&[
        "new",
        "behavior",
        "archive_task",
        "Archive a Task",
        "--in",
        "behaviors/task.spec",
        "--contract",
        "When asked, the system MUST archive.",
        "--dry-run",
    ]);
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout
            .contains("+behavior archive_task \"Archive a Task\" {"),
        "{}",
        run.stdout
    );
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new refuses a field given twice that takes one value"
)]
fn new_given_one_value_field_twice_is_refused() {
    let project = todo();
    let run = sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive",
            "--in",
            "behaviors/task.spec",
            "--contract",
            "first",
            "--contract",
            "second",
            "--dry-run",
        ],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("takes one text"), "{}", run.stderr);
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new refuses an unknown kind as unknown_kind"
)]
fn new_of_an_unknown_kind_is_refused_as_unknown_kind() {
    let project = todo();
    let run = sf(
        &project,
        &[
            "new",
            "behaviour",
            "archive_task",
            "Archive",
            "--in",
            "behaviors/task.spec",
            "--contract",
            "x",
        ],
    );
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("unknown_kind"), "{}", run.stderr);
    assert!(
        run.stderr.contains("did you mean 'behavior'?"),
        "{}",
        run.stderr
    );
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new --help begins with what new does"
)]
#[ignore = "flipped by T3"]
fn new_help_begins_with_what_new_does() {
    let project = todo();
    let help = sf(&project, &["new", "--help"]).exits(0);
    assert_eq!(
        help.stdout.lines().next(),
        Some("Create an entity"),
        "{}",
        help.stdout
    );
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new refuses one of its options given twice, wherever each stands"
)]
#[ignore = "flipped by T3"]
fn an_option_of_new_given_twice_is_refused_wherever_it_stands() {
    let project = todo();
    let before = project.read(TASKS);
    let twice_in = sf(
        &project,
        &[
            "new",
            "--in",
            "behaviors/a.spec",
            "behavior",
            "archive_task",
            "Archive",
            "--contract",
            "c",
            "--in",
            "behaviors/b.spec",
            "--dry-run",
        ],
    );
    let twice_format = sf(
        &project,
        &[
            "new",
            "--format",
            "json",
            "behavior",
            "archive_task",
            "Archive",
            "--contract",
            "c",
            "--in",
            "behaviors/task.spec",
            "--dry-run",
            "--format",
            "human",
        ],
    );
    for run in [&twice_in, &twice_format] {
        assert_eq!(run.code, 2, "{}{}", run.stdout, run.stderr);
        assert!(
            run.stderr.contains("cannot be used multiple times"),
            "{}",
            run.stderr
        );
    }
    assert_eq!(project.read(TASKS), before);
    for absent in ["spec/behaviors/a.spec", "spec/behaviors/b.spec"] {
        assert!(!project.root().join(absent).exists(), "{absent}");
    }
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new's help of a kind shows each of its options as every command declares it"
)]
#[ignore = "flipped by T3"]
fn the_kinds_help_shows_each_option_as_declared() {
    let project = todo();
    let help = sf(&project, &["new", "behavior", "--help"]).exits(0);
    for text in [
        "--path <PATH>",
        "the nearest directory at or above it holding specforge.json",
        "[default: .]",
        "Output format: human or json",
        "[default: human]",
        "-h, --help",
        "created when missing",
    ] {
        assert!(help.stdout.contains(text), "{text}\n{}", help.stdout);
    }
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new --help of a kind the project does not declare is refused as unknown_kind"
)]
#[ignore = "flipped by T3"]
fn help_of_an_unknown_kind_is_refused_as_unknown_kind() {
    let project = todo();
    let run = sf(&project, &["new", "behaviour", "--help"]);
    assert_eq!(run.code, 1, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("unknown_kind"), "{}", run.stderr);
    assert!(
        run.stderr.contains("did you mean 'behavior'?"),
        "{}",
        run.stderr
    );
    assert!(run.stdout.is_empty(), "{}", run.stdout);
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new --help after a field flag prints the kind's help"
)]
#[ignore = "flipped by T3"]
fn help_after_a_field_flag_is_the_kinds_help() {
    let project = todo();
    let before = project.read(TASKS);
    let help = sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive",
            "--contract",
            "c",
            "--help",
        ],
    )
    .exits(0);
    assert!(
        help.stdout.contains("--contract <VALUE>"),
        "{}",
        help.stdout
    );
    assert_eq!(project.read(TASKS), before);
}

#[specforge_test(
    behavior = "create_entity",
    verify = "new refuses a command line whose --path a field's value spells"
)]
#[ignore = "flipped by T3"]
fn a_field_value_spelled_as_path_names_no_project() {
    let project = todo();
    let other = todo();
    let spelled = format!("--path={}", other.root().display());
    let run = sf(
        &project,
        &[
            "new",
            "behavior",
            "archive_task",
            "Archive",
            "--contract",
            &spelled,
            "--in",
            "behaviors/x.spec",
        ],
    );
    assert_eq!(run.code, 2, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("--path"), "{}", run.stderr);
    for root in [other.root(), project.root()] {
        assert!(!root.join("spec/behaviors/x.spec").exists(), "{root:?}");
    }
}
