//! What the server answers while its session is out for an update (ADR 0100 D4, ADR 0108): the
//! readers see a copy of the session's last complete compile.

use crate::hover::coverage_project;
use crate::served::hover_text;
use specforge_test_macros::test as spec;

/// The hover's Coverage line, if it has one.
fn coverage_line(hover: &str) -> Option<String> {
    hover
        .split("\n\n---\n\n")
        .find(|section| section.starts_with("**Coverage**"))
        .map(str::to_string)
}

#[spec(
    behavior = "provide_extension_entity_hover",
    verify = "hover shows the last complete compile's coverage while the project rebuilds"
)]
fn hover_shows_the_last_compiles_coverage_while_the_session_is_out() {
    let mut project = coverage_project("behavior done \"Done\" {\n  verify unit \"first\"\n}\n");
    project.write(
        "specforge-report.json",
        r#"{"results": {"done": {"tests": [{"name": "t", "status": "passed", "verify": "first"}]}}}"#,
    );
    let uri = project.uri("main.spec");
    let position = project.position_of("main.spec", "done");
    let held = hover_text(&project.state(), &uri, position).unwrap();
    let out = project
        .rebuilding(|state| hover_text(state, &uri, position))
        .unwrap();
    assert_eq!(
        coverage_line(&held).as_deref(),
        Some("**Coverage** · `covered` · 1/1 obligations proven · 1 test")
    );
    assert_eq!(coverage_line(&out), coverage_line(&held));
}

#[test]
fn the_view_while_out_is_rooted_where_the_session_is() {
    let mut served = coverage_project("behavior done \"Done\" {\n}\n");
    let held = served
        .state()
        .view()
        .root()
        .map(std::path::Path::to_path_buf);
    let out = served.rebuilding(|state| state.view().root().map(std::path::Path::to_path_buf));
    assert!(held.is_some());
    assert_eq!(out, held);
}
