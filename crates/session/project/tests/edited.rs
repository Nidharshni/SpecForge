//! `CompiledProject::edited`: the project as if some sources held other text, which a typed edit
//! validates before it writes (ADR 0151).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use specforge_project::testing::TestProject;
use specforge_project::{ProjectSession, RuntimeSource};
use specforge_registry::{Disk, PlainDisk};
use specforge_test::prelude::*;

const A: &str = "behavior alpha \"A\" {\n  contract \"\"\"\n    A MUST work.\n  \"\"\"\n}\n";

fn project() -> TestProject {
    TestProject::new()
        .enabling(&["@specforge/software", "@specforge/product"])
        .files(&[("alpha.spec", A)])
}

fn texts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, t)| (k.to_string(), t.to_string()))
        .collect()
}

fn errors(project: &specforge_project::CompiledProject) -> Vec<String> {
    project
        .diagnostics()
        .iter()
        .filter(|d| d.code.as_str() == "E003")
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn an_edited_text_replaces_the_compiled_one() {
    let dir = project();
    let compiled = dir.compile();
    assert!(errors(&compiled).is_empty());
    let dangling = A.replace("}\n", "  features [nope_feature]\n}\n");
    let edited = compiled.edited(&texts(&[("alpha.spec", &dangling)]));
    assert_eq!(errors(&edited).len(), 1, "{:?}", edited.diagnostics());
    assert!(
        errors(&compiled).is_empty(),
        "the compiled project is unchanged"
    );
}

#[test]
fn a_new_source_key_is_compiled() {
    let dir = project();
    let compiled = dir.compile();
    let edited = compiled.edited(&texts(&[("new.spec", "term fresh_one \"Fresh\" {\n}\n")]));
    assert!(
        edited
            .graph()
            .nodes()
            .iter()
            .any(|n| n.id.raw.as_str() == "fresh_one")
    );
    assert!(
        !compiled
            .graph()
            .nodes()
            .iter()
            .any(|n| n.id.raw.as_str() == "fresh_one")
    );
}

#[test]
fn a_key_outside_the_sources_is_not_compiled() {
    let dir = project();
    let edited = dir.compile().edited(&texts(&[
        ("../x.spec", "term x1 \"X\" {\n}\n"),
        ("notes.txt", "term x2 \"X\" {\n}\n"),
        ("target/y.spec", "term x3 \"X\" {\n}\n"),
    ]));
    assert!(
        !edited
            .graph()
            .nodes()
            .iter()
            .any(|n| n.id.raw.as_str().starts_with('x'))
    );
    assert_eq!(edited.source_keys().len(), 1);
}

#[test]
fn edited_reads_no_disk_for_unedited_sources() {
    let dir = project();
    let compiled = dir.compile();
    dir.write("alpha.spec", "this is not a spec {{{");
    let edited = compiled.edited(&texts(&[("b.spec", "term bee \"Bee\" {\n}\n")]));
    assert_eq!(
        edited.source_text("alpha.spec").as_deref(),
        Some(A),
        "alpha.spec is the text the project was compiled from"
    );
}

#[test]
fn edited_reports_what_check_reports_after_the_write() {
    let dir = project();
    let compiled = dir.compile();
    let dangling = A.replace("}\n", "  features [nope_feature]\n}\n");
    let edited = compiled.edited(&texts(&[
        ("alpha.spec", &dangling),
        ("b.spec", "term bee \"Bee\" {\n}\n"),
    ]));
    dir.write("alpha.spec", &dangling);
    dir.write("b.spec", "term bee \"Bee\" {\n}\n");
    assert_eq!(edited.diagnostics(), dir.compile().diagnostics());
}

/// Refuses `leak.spec` as a bound refuses a source that leaves the project; reads everything else plainly.
struct RefusesLeak;

impl Disk for RefusesLeak {
    fn exists(&self, path: &Path) -> bool {
        PlainDisk.exists(path)
    }
    fn list(&self, dir: &Path) -> Option<Vec<String>> {
        PlainDisk.list(dir)
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path.ends_with("leak.spec") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refused by the bound",
            ));
        }
        PlainDisk.read(path)
    }
}

const LEAK: &str = "use \"SECRETWORD_IMPORT\"\nbehavior alpha \"SECRETWORD_TITLE\" {\n}\n";

#[specforge_test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn an_edited_compile_reads_no_source_the_project_could_not_read() {
    let dir = TestProject::new().files(&[
        ("a.spec", "behavior alpha \"Alpha\" {\n}\n"),
        ("leak.spec", LEAK),
    ]);
    let session = ProjectSession::open_through(
        &dir.project_root(),
        RuntimeSource::Fixed(None),
        Some(Arc::new(RefusesLeak)),
    );
    let project = session.project();
    let before = format!("{:?}", project.diagnostics());
    assert!(before.contains("cannot read leak.spec: refused by the bound"));
    assert!(!before.contains("SECRETWORD"));
    let edited = project.edited(&texts(&[("a.spec", "behavior alpha \"Alpha Two\" {\n}\n")]));
    assert!(!format!("{:?}", edited.diagnostics()).contains("SECRETWORD"));
    assert_eq!(edited.diagnostics(), project.diagnostics());
}

#[test]
fn an_edited_compile_reports_what_holding_its_texts_reports() {
    let dir = project().files(&[(
        "b.spec",
        "behavior bee \"B\" {\n  contract \"\"\"\n    B MUST work.\n  \"\"\"\n}\n",
    )]);
    let mut session = dir.session_in(dir.runtime());
    let dangling = A.replace("}\n", "  features [nope_feature]\n}\n");
    let fresh = "term bee_term \"Bee\" {\n}\n";
    let edited = session
        .project()
        .edited(&texts(&[("alpha.spec", &dangling), ("b.spec", fresh)]));
    let root = dir.root();
    session.hold(&[
        specforge_project::Buffer::new(root.join("alpha.spec"), dangling.clone()),
        specforge_project::Buffer::new(root.join("b.spec"), fresh),
    ]);
    assert_eq!(edited.diagnostics(), session.project().diagnostics());
}

#[specforge_test_macros::test(
    behavior = "validate_typed_edit",
    verify = "an edit planned while the checks are skipped is compared with the checked report"
)]
fn the_checked_report_runs_the_checks_an_update_skipped() {
    let lonely = "behavior lonely \"Lonely\" {\n  category query\n}\n";
    let dir = project().files(&[("lonely.spec", lonely)]);
    let has_e006 = |d: &[specforge_common::Diagnostic]| d.iter().any(|d| d.code == "E006");
    let oneshot = dir.compile();
    assert!(has_e006(&oneshot.diagnostics()));
    assert_eq!(oneshot.checked_diagnostics(), oneshot.diagnostics());

    let mut session = dir.session_in(dir.runtime());
    session.hold_with(
        &[specforge_project::Buffer::new(
            dir.root().join("alpha.spec"),
            "behavior alpha \"A\" {\n",
        )],
        specforge_project::CheckMode::SyntaxOnlyIfParseErrors,
    );
    let project = session.project();
    assert!(!has_e006(&project.diagnostics()), "the checks were skipped");
    assert!(has_e006(&project.checked_diagnostics()));
    assert_eq!(
        project.checked_diagnostics(),
        project.edited(&BTreeMap::new()).diagnostics()
    );
    assert!(!has_e006(&project.diagnostics()), "no side effect");
}

#[test]
fn an_edited_project_keeps_its_answer_to_rooted() {
    let assembled = specforge_project::testing::Assembled::new().source(A);
    assert_eq!(assembled.detached().edited(&BTreeMap::new()).root(), None);
    assert_eq!(
        assembled.compiled().edited(&BTreeMap::new()).root(),
        Some(assembled.root())
    );
}
