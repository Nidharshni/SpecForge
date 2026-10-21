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

#[test]
#[ignore = "flipped by T3"]
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

#[test]
#[ignore = "flipped by T3"]
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
