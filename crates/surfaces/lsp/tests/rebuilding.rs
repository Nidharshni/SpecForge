//! What the server answers while its session is out for an update (ADR 0100 D4, ADR 0108): the
//! readers see a copy of the session's last complete compile.

use crate::hover::coverage_project;
use crate::served::hover_text;

/// The hover's Coverage line, if it has one.
fn coverage_line(hover: &str) -> Option<String> {
    hover
        .split("\n\n---\n\n")
        .find(|section| section.starts_with("**Coverage**"))
        .map(str::to_string)
}

#[test]
#[ignore = "flipped by T5"]
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
