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
    assert!(!planned.change().is_empty());
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
    assert!(!planned.change().is_empty());
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "an edit that adds a warning is written and reports it"
)]
fn an_added_warning_is_written_and_reported() {
    let c = tasks();
    let planned = plan(&c, &[unset("list_tasks", "category")]).unwrap();
    assert!(!planned.change().is_empty());
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
    for (file, text) in planned.change().after() {
        assert!(is_canonical(&c, &text), "{file}");
    }
}

/// The editor's typing fast path skipped the checks (ADR 0136 D4): a plan is not refused for the error another file
/// already had.
#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "an edit planned while the checks are skipped is compared with the checked report"
)]
fn an_edit_planned_while_the_checks_are_skipped_is_not_refused_for_an_error_the_project_had() {
    use specforge_project::testing::TestProject;
    use specforge_project::{Buffer, CheckMode};
    let dir = TestProject::new()
        .enabling(&["@specforge/software"])
        .files(&[
            ("tasks.spec", TASKS),
            (
                "lonely.spec",
                "behavior lonely \"Lonely\" {\n  category query\n}\n",
            ),
            (
                "draft.spec",
                "behavior draft \"Draft\" {\n  category query\n}\n",
            ),
        ]);
    let mut session = dir.session_in(dir.runtime());
    assert!(
        session
            .project()
            .diagnostics()
            .iter()
            .any(|d| d.code == "E006"),
        "{:?}",
        session.project().diagnostics()
    );
    session.hold_with(
        &[Buffer::new(
            dir.root().join("draft.spec"),
            "behavior draft \"Draft\" {\n",
        )],
        CheckMode::SyntaxOnlyIfParseErrors,
    );
    assert!(
        !session
            .project()
            .diagnostics()
            .iter()
            .any(|d| d.code == "E006"),
        "the checks were skipped"
    );
    let view = ProjectView::of(session.project());
    let planned = edit::plan(&view, &[set("list_tasks", "category", one("command"))]);
    assert!(
        planned.is_ok(),
        "{:?}",
        planned.err().map(|r| r.error.message)
    );
}

/// `TASKS` with its last block written twice: one E002, whose message names the line of the first.
fn with_duplicate_id() -> String {
    let last = TASKS.rfind("behavior list_tasks").unwrap();
    format!("{TASKS}\n{}", &TASKS[last..])
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "an error whose message names a position is the same error after an edit moves it"
)]
fn an_edit_above_a_duplicate_id_does_not_count_it_as_new() {
    let c = project(&[("tasks.spec", &with_duplicate_id())]);
    assert!(c.project.diagnostics().iter().any(|d| d.code == "E002"));
    // The edit removes a line above the duplicate: its message now names another line, the error is the same.
    let planned = plan(&c, &[unset("complete_task", "category")]);
    assert!(planned.is_ok(), "{:?}", planned.err().map(|e| e.error));
}

#[test]
fn a_second_error_that_differs_in_digits_is_still_new() {
    // `ghost1` is unresolved already; `ghost2` has the same digit-collapsed message: two against one.
    let broken = TASKS.replace("[done_stamped]", "[done_stamped, ghost1]");
    let c = project(&[("tasks.spec", &broken)]);
    assert!(c.project.diagnostics().iter().any(|d| d.code == "E003"));
    let error = plan(&c, &[link("complete_task", "invariants", "ghost2")])
        .unwrap_err()
        .error;
    assert!(error.is(codes::E080), "{error:?}");
    assert!(
        error.message.contains("1 error it does not have"),
        "{}",
        error.message
    );
}
