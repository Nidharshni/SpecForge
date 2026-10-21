//! A spec file a request names is read into its source key once (ADR 0195): a typed create's `file`, outline's
//! `file`, the fixes' `file_path`.

use crate::edit::{TASKS, create, one};
use crate::navigate::{LIMIT, compile};
use specforge_ops::authoring::edit::{self, Edit, EditPlan};
use specforge_ops::authoring::fix::{self, FixQuery};
use specforge_ops::view::ProjectView;
use specforge_project::testing::compile_at;

/// A project whose spec root is `spec/`, holding `files` (named from that root), and the plan of `edits` over it.
fn plan_in_spec_dir(files: &[(&str, &str)], edits: &[Edit]) -> EditPlan {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "n", "spec_root": "spec", "extensions": ["@specforge/software"]})
            .to_string(),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("spec")).unwrap();
    for (name, text) in files {
        std::fs::write(dir.path().join("spec").join(name), text).unwrap();
    }
    let project = compile_at(
        dir.path(),
        Some(specforge_project::testing::component_runtime()),
    );
    edit::plan(&ProjectView::of(&project), edits).unwrap()
}

const TESTING: &[&str] = &["@specforge/software", "@specforge/testing"];

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "new reads every spelling of a file under the spec root as that file"
)]
fn a_create_in_a_spelling_of_an_existing_file_adds_to_it() {
    let planned = plan_in_spec_dir(
        &[("tasks.spec", TASKS)],
        &[create(
            "behavior",
            "archive_task",
            "./tasks.spec",
            &[("contract", one("The system MUST archive."))],
        )],
    );
    assert_eq!(planned.report().affected_files, ["tasks.spec"]);
    assert!(planned.change().after()["tasks.spec"].starts_with(TASKS));
}

#[specforge_test_macros::test(
    behavior = "create_entity",
    verify = "two spellings of one new file in one list are one file"
)]
fn two_spellings_of_one_new_file_are_one_file() {
    let planned = plan_in_spec_dir(
        &[("tasks.spec", TASKS)],
        &[
            create(
                "behavior",
                "alpha_one",
                "./n.spec",
                &[("contract", one("The system MUST archive."))],
            ),
            create(
                "behavior",
                "beta_one",
                "n.spec",
                &[("contract", one("The system MUST archive."))],
            ),
        ],
    );
    assert_eq!(planned.report().affected_files, ["n.spec"]);
    let after = &planned.change().after()["n.spec"];
    assert!(
        after.contains("behavior alpha_one") && after.contains("behavior beta_one"),
        "{after}"
    );
}

const LOGOUT: &str =
    "behavior login \"Login\" {\n  contract \"c\"\n  invariants [sesion_limit]\n}\n";

#[test]
#[ignore = "flipped by T5"]
fn the_fixes_read_a_spelling_of_a_source_as_that_source() {
    let p = compile(TESTING, &[("limit.spec", LIMIT), ("login.spec", LOGOUT)]);
    let titles = |file: &str| -> Vec<String> {
        let query = FixQuery {
            file: Some(file),
            ..FixQuery::default()
        };
        fix::fixes(&p.navigator(), &p.project.diagnostics(), &query)
            .unwrap()
            .into_iter()
            .map(|f| f.title)
            .collect()
    };
    let exact = titles("login.spec");
    assert!(!exact.is_empty());
    let absolute = p.project.environment().spec_root().join("login.spec");
    assert_eq!(titles("./login.spec"), exact);
    assert_eq!(titles(absolute.to_str().unwrap()), exact);
}

#[test]
#[ignore = "flipped by T5"]
fn the_fixes_refuse_a_file_that_is_no_source() {
    let p = compile(TESTING, &[("limit.spec", LIMIT)]);
    let refusal = |file: &str| {
        let query = FixQuery {
            file: Some(file),
            ..FixQuery::default()
        };
        fix::fixes(&p.navigator(), &p.project.diagnostics(), &query)
            .unwrap_err()
            .error
            .code
            .to_string()
    };
    assert_eq!(refusal("nope.spec"), "file_not_found");
    assert_eq!(refusal("../x.spec"), "not_a_source");
}

const A: &str = "invariant rule_a \"A\" {\n  guarantee \"x\"\n}\n";
const B: &str = "invariant rule_b \"B\" {\n  guarantee \"x\"\n}\n";

/// A project with `spec_root` `spec/`, holding `a.spec` and `sub/b.spec`, compiled.
fn spec_dir_project() -> (tempfile::TempDir, specforge_project::CompiledProject) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        serde_json::json!({"name": "n", "spec_root": "spec", "extensions": ["@specforge/software"]})
            .to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("spec/sub")).unwrap();
    std::fs::write(dir.path().join("spec/a.spec"), A).unwrap();
    std::fs::write(dir.path().join("spec/sub/b.spec"), B).unwrap();
    let project = compile_at(
        dir.path(),
        Some(specforge_project::testing::component_runtime()),
    );
    (dir, project)
}

fn kind_and_code(error: &specforge_ops::OpError) -> (specforge_ops::OpErrorKind, &str) {
    (error.kind, &error.code)
}

#[specforge_test_macros::test(
    behavior = "read_a_named_spec_file",
    verify = "every spelling of a spec file under the spec root reads as its source key"
)]
fn the_view_reads_every_spelling_into_its_source_key() {
    let (_dir, project) = spec_dir_project();
    let view = ProjectView::of(&project);
    let spec_root = project.environment().spec_root().to_path_buf();
    let absolute = spec_root.join("a.spec");
    for (named, key) in [
        ("a.spec", "a.spec"),
        ("./a.spec", "a.spec"),
        (".//a.spec", "a.spec"),
        (absolute.to_str().unwrap(), "a.spec"),
        ("sub/./b.spec", "sub/b.spec"),
        ("sub//b.spec", "sub/b.spec"),
    ] {
        assert_eq!(view.source(named).unwrap(), key, "{named}");
        assert_eq!(view.source_place(named).unwrap(), key, "{named}");
    }
    // The LSP's and the stub fix's keys read as themselves.
    for key in project.source_keys() {
        assert_eq!(view.source(key).unwrap(), key);
        assert_eq!(view.source_place(key).unwrap(), key);
    }
    assert_eq!(view.source_place("new/c.spec").unwrap(), "new/c.spec");
}

#[specforge_test_macros::test(
    behavior = "read_a_named_spec_file",
    verify = "a spelling that leaves the spec root is not_a_source"
)]
fn the_view_refuses_a_spelling_outside_the_spec_root() {
    let (_dir, project) = spec_dir_project();
    let view = ProjectView::of(&project);
    for named in ["../x.spec", "sub/../a.spec", "/elsewhere/x.spec"] {
        for refused in [view.source(named), view.source_place(named)] {
            let error = refused.unwrap_err();
            assert_eq!(
                kind_and_code(&error),
                (specforge_ops::OpErrorKind::InvalidInput, "not_a_source"),
                "{named}"
            );
        }
    }
}

#[specforge_test_macros::test(
    behavior = "read_a_named_spec_file",
    verify = "a read of a file that is no compiled source is file_not_found"
)]
fn the_view_reads_no_source_as_file_not_found() {
    let (_dir, project) = spec_dir_project();
    let view = ProjectView::of(&project);
    for named in ["nope.spec", "", ".", "specforge.json", "notes.txt"] {
        let error = view.source(named).unwrap_err();
        assert_eq!(
            kind_and_code(&error),
            (specforge_ops::OpErrorKind::FileNotFound, "file_not_found"),
            "{named:?}"
        );
    }
    for named in ["notes.txt", "", "node_modules/x.spec"] {
        let error = view.source_place(named).unwrap_err();
        assert_eq!(kind_and_code(&error).1, "not_a_source", "{named:?}");
    }
}

#[specforge_test_macros::test(
    behavior = "read_a_named_spec_file",
    verify = "a spec file named from the project root is refused naming its spec-root spelling"
)]
fn the_view_refuses_a_project_root_spelling_naming_its_key() {
    let (_dir, project) = spec_dir_project();
    let view = ProjectView::of(&project);
    let error = view.source("spec/a.spec").unwrap_err();
    assert_eq!(error.kind, specforge_ops::OpErrorKind::FileNotFound);
    assert!(
        error.suggestion.as_deref().unwrap().contains("'a.spec'"),
        "{error:?}"
    );
    let error = view.source_place("spec/a.spec").unwrap_err();
    assert_eq!(error.code, "not_a_source");
    assert!(
        error.suggestion.as_deref().unwrap().contains("'a.spec'"),
        "{error:?}"
    );
    assert_eq!(view.source_place("spec/new.spec").unwrap(), "spec/new.spec");
}
