//! A document names each file an operation opened or wrote from the project root, whatever the working
//! directory or the spelling of the root (ADR 0102). Each test runs the binary as a child, one per working directory:
//! the process's own is never changed.

use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;
use std::fs;
use std::path::Path;

/// A project whose `spec/bad.spec` formatting changes and whose last line cannot be parsed (a W142).
fn project_with_a_bad_file() -> TestProject {
    let project = TestProject::new().raw_config(r#"{"name":"test","version":"0.1.0"}"#);
    fs::create_dir_all(project.root().join("spec")).unwrap();
    fs::write(
        project.root().join("spec/bad.spec"),
        "behavior bar \"Bar\" {\n      contract \"stuff\"\n}\n\n}}}\n",
    )
    .unwrap();
    project
}

/// `specforge <args>` from `dir`, no implied `--path`: the document it prints.
fn document_from(project: &TestProject, dir: &Path, args: &[&str]) -> Value {
    Cli::on(project).in_dir(dir).bare(args).json()
}

#[cfg(unix)]
#[specforge_test(
    behavior = "format_spec_files",
    verify = "the format document names each file from the project root, whatever the working directory"
)]
#[ignore = "flipped by T4"]
fn format_document_names_a_named_file_from_the_project_root_from_any_cwd() {
    let project = project_with_a_bad_file();
    let root = project.root().to_path_buf();
    let parent = root.parent().unwrap().to_path_buf();
    let name = root.file_name().unwrap().to_str().unwrap().to_string();
    let link_dir = tempfile::TempDir::new().unwrap();
    let link = link_dir.path().join("link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    let (root_s, link_s) = (root.to_str().unwrap(), link.to_str().unwrap());
    let absolute = format!("{root_s}/spec/bad.spec");
    let through_link = format!("{link_s}/spec/bad.spec");
    let from_parent = format!("{name}/spec/bad.spec");

    let runs = [
        (root.clone(), vec!["spec/bad.spec", "--path", root_s]),
        (root.join("spec"), vec!["bad.spec", "--path", root_s]),
        (parent, vec![from_parent.as_str(), "--path", name.as_str()]),
        (root.clone(), vec![absolute.as_str(), "--path", root_s]),
        (
            link_dir.path().to_path_buf(),
            vec![through_link.as_str(), "--path", link_s],
        ),
    ];
    for (dir, named) in runs {
        let mut args = vec!["format"];
        args.extend(named.iter().copied());
        args.extend(["--diff", "--format", "json"]);
        let document = document_from(&project, &dir, &args);

        assert_eq!(
            document["changed_files"],
            json!(["spec/bad.spec"]),
            "{args:?} {document}"
        );
        assert_eq!(
            document["diffs"][0]["file_path"], "spec/bad.spec",
            "{args:?}"
        );
        let warning = document["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "W142")
            .unwrap_or_else(|| panic!("a W142: {document}"));
        assert_eq!(warning["span"]["file"], "spec/bad.spec", "{args:?}");
        assert_eq!(warning["file"], "spec/bad.spec", "{args:?}");
    }
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "the format document names each file from the project root, whatever the working directory"
)]
#[ignore = "flipped by T4"]
fn format_failures_name_the_file_from_the_project_root() {
    let project = project_with_a_bad_file();
    let root = project.root().to_path_buf();

    let document = document_from(
        &project,
        &root.join("spec"),
        &[
            "format",
            "nope.spec",
            "--path",
            root.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    assert_eq!(
        document["failed_files"],
        json!(["spec/nope.spec"]),
        "{document}"
    );
    assert_eq!(
        document["failures"][0]["file"], "spec/nope.spec",
        "{document}"
    );
    for message in [&document["message"], &document["failures"][0]["message"]] {
        assert!(
            message
                .as_str()
                .unwrap()
                .starts_with("failed to read spec/nope.spec: "),
            "{document}"
        );
    }
}

#[specforge_test(
    behavior = "format_spec_files",
    verify = "the format document names each file from the project root, whatever the working directory"
)]
fn files_written_outside_the_root_is_absolute_and_canonical() {
    let project = project_with_a_bad_file();
    let root = project.root().to_path_buf();
    let other = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(other.path().join("sub")).unwrap();
    fs::write(
        other.path().join("x.spec"),
        "behavior bar \"Bar\" {\n      contract \"stuff\"\n}\n",
    )
    .unwrap();
    let named = other.path().join("sub/../x.spec");

    let document = document_from(
        &project,
        &root,
        &[
            "format",
            named.to_str().unwrap(),
            "--path",
            root.to_str().unwrap(),
            "--format",
            "json",
        ],
    );

    let canonical = fs::canonicalize(other.path().join("x.spec")).unwrap();
    assert_eq!(
        document["files_written"],
        json!([canonical.to_str().unwrap()]),
        "{document}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_collect_tool",
    verify = "the collect document names the report from the project root"
)]
#[ignore = "flipped by T5"]
fn collect_report_is_named_from_the_project_root() {
    let project = TestProject::new().raw_config(
        r#"{"name":"col","spec_root":"spec","extensions":["@specforge/software","@specforge/testing","@specforge/cargo-test"]}"#,
    );
    let root = project.root();
    fs::create_dir_all(root.join("spec")).unwrap();
    fs::create_dir_all(root.join("target/specforge")).unwrap();
    fs::write(
        root.join("spec/a.spec"),
        "behavior alpha \"Alpha\" {\n  verify unit \"works\"\n}\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::write(
        root.join("target/specforge/demo.json"),
        json!({"entries": [
            {"entity_id": "alpha", "test_name": "works", "verify": "works", "status": "passed"},
        ]})
        .to_string(),
    )
    .unwrap();

    let document = Cli::on(&project).document(&["collect", "--no-run"]).json;

    assert_eq!(document["report"], "specforge-report.json", "{document}");
}

#[specforge_test(
    behavior = "migrate_spec_files_in_place",
    verify = "the migrate document names each file and backup from the project root"
)]
#[ignore = "flipped by T6"]
fn migrate_names_files_and_backups_from_the_project_root() {
    let project = TestProject::new().raw_config(r#"{"name":"test","version":"0.1.0"}"#);
    let root = project.root();
    fs::create_dir_all(root.join("spec")).unwrap();
    fs::write(
        root.join("spec/a.spec"),
        "// specforge-format: 0.1\nbehavior foo \"Foo\" {\n  contract \"stuff\"\n}\n",
    )
    .unwrap();

    let document = Cli::on(&project).document(&["migrate"]).json;

    assert_eq!(
        document["results"][0]["file_path"], "spec/a.spec",
        "{document}"
    );
    assert_eq!(
        document["backups"][0],
        json!({"original_path": "spec/a.spec", "backup_path": "spec/a.spec.bak"}),
        "{document}"
    );
}
