use super::*;
use specforge_common::codes;

fn tasks() -> Compiled {
    project(&[("tasks.spec", TASKS)])
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "an edit that adds an error is E080 and writes nothing"
)]
fn a_dangling_link_is_e080_with_the_e003_in_its_data() {
    let c = tasks();
    let error = plan(&c, &[link("complete_task", "invariants", "nope_rule")])
        .unwrap_err()
        .error;
    assert!(error.is(codes::E080), "{error:?}");
    assert_eq!(error.kind, specforge_ops::OpErrorKind::CompilationFailed);
    assert!(
        error.message.contains("1 error it does not have"),
        "{}",
        error.message
    );
    assert!(
        error
            .message
            .contains("error[E003]: unresolved reference 'nope_rule'")
    );
    let data = error.data.unwrap();
    assert_eq!(data["diagnostics"][0]["code"], "E003");
}

#[test]
fn a_missing_required_field_is_e080() {
    let c = tasks();
    let error = plan(&c, &[unset("list_tasks", "contract")])
        .unwrap_err()
        .error;
    assert!(error.is(codes::E080));
    assert!(error.message.contains("E006"), "{}", error.message);
}

#[test]
fn an_error_the_project_already_had_does_not_refuse() {
    let broken = TASKS.replace(
        "  contract \"\"\"\n    When asked, the system MUST list.\n  \"\"\"\n",
        "",
    );
    let c = project(&[("tasks.spec", &broken)]);
    assert!(
        c.project.diagnostics().iter().any(|d| d.code == "E006"),
        "the project already has an error"
    );
    let planned = plan(&c, &[link("complete_task", "invariants", "task_unique")]).unwrap();
    assert!(!planned.is_empty());
}

#[test]
fn an_inserted_block_does_not_count_moved_errors_as_new() {
    let broken = TASKS.replace(
        "  contract \"\"\"\n    When asked, the system MUST list.\n  \"\"\"\n",
        "",
    );
    let c = project(&[("tasks.spec", &broken)]);
    // A line added above the error moves it; it is the same error.
    let planned = plan(
        &c,
        &[
            set("complete_task", "category", one("query")),
            link("complete_task", "invariants", "extra_rule"),
        ],
    )
    .unwrap();
    assert!(!planned.is_empty());
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "an edit that adds a warning is written and reports it"
)]
fn an_added_warning_is_written_and_reported() {
    let c = tasks();
    let planned = plan(&c, &[unset("list_tasks", "category")]).unwrap();
    assert!(!planned.is_empty());
    let added: Vec<&str> = planned
        .report()
        .added
        .0
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(added, vec!["W006"]);
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "a preview refuses what the write would"
)]
fn a_preview_refuses_what_the_write_would() {
    let c = tasks();
    // The plan is the preview and the write's input: one validation, one answer.
    assert!(plan(&c, &[link("complete_task", "invariants", "nope_rule")]).is_err());
    let report = plan(&c, &[link("list_tasks", "invariants", "task_unique")]).unwrap();
    assert!(
        report.report().diff.contains("+  invariants"),
        "{}",
        report.report().diff
    );
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "every block an edit touches is format-clean"
)]
fn every_planned_file_is_format_clean() {
    let c = tasks();
    let planned = plan(
        &c,
        &[
            link("list_tasks", "invariants", "task_unique"),
            link("complete_task", "invariants", "extra_rule"),
        ],
    )
    .unwrap();
    for (file, text) in planned.after() {
        assert!(is_canonical(&c, &text), "{file}");
    }
}
