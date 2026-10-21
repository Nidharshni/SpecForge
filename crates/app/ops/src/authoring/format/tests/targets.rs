use super::*;

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "no arguments formats all .spec files under spec_root"
)]
#[specforge_test(
    invariant = "discover_completeness",
    verify = "all .spec files under spec_root are discovered"
)]
fn targets_are_the_files_under_spec_root() {
    let dir = project_with(
        r#"{"spec_root": "specs"}"#,
        &[
            ("specs/a.spec", MESSY),
            ("specs/sub/b.spec", MESSY),
            ("fixtures/fx.spec", MESSY),
        ],
    );
    assert_eq!(changed(dir.path()), ["specs/a.spec", "specs/sub/b.spec"]);

    // A spec/ directory beside the spec root is no source either.
    std::fs::create_dir(dir.path().join("spec")).unwrap();
    std::fs::write(dir.path().join("spec/old.spec"), MESSY).unwrap();
    assert_eq!(changed(dir.path()), ["specs/a.spec", "specs/sub/b.spec"]);

    // Without spec_root, every source under the root, as check reads them.
    std::fs::write(dir.path().join("specforge.json"), "{}").unwrap();
    assert_eq!(
        changed(dir.path()),
        [
            "fixtures/fx.spec",
            "spec/old.spec",
            "specs/a.spec",
            "specs/sub/b.spec"
        ]
    );
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "files the project's exclude entries leave out are not formatted"
)]
fn excluded_files_are_not_formatted() {
    let dir = project_with(
        r#"{"spec_root": "spec", "exclude": ["drafts"]}"#,
        &[("spec/a.spec", MESSY), ("spec/drafts/d.spec", MESSY)],
    );

    assert_eq!(changed(dir.path()), ["spec/a.spec"]);
    // A file named explicitly is formatted all the same.
    let named = check_paths(dir.path(), &["spec/drafts/d.spec"]);
    assert_eq!(changed_in(dir.path(), &named), ["spec/drafts/d.spec"]);
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "explicit file paths format only those files"
)]
fn explicit_files_format_only_those() {
    let dir = project();

    let outcome = check_paths(dir.path(), &["spec/a.spec"]);

    assert_eq!(outcome.checked, 1);
    assert_eq!(changed_in(dir.path(), &outcome), ["spec/a.spec"]);
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "directory argument recursively discovers .spec files"
)]
fn a_directory_argument_is_walked() {
    let dir = project_with(
        r#"{"exclude": ["drafts"]}"#,
        &[
            ("spec/sub/a.spec", MESSY),
            ("spec/sub/deep/b.spec", MESSY),
            ("spec/sub/drafts/d.spec", MESSY),
            ("spec/sub/target/t.spec", MESSY),
            ("spec/other.spec", MESSY),
        ],
    );

    let outcome = check_paths(dir.path(), &["spec/sub"]);

    assert_eq!(
        changed_in(dir.path(), &outcome),
        ["spec/sub/a.spec", "spec/sub/deep/b.spec"]
    );
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "non-.spec files are skipped with no error"
)]
fn non_spec_files_are_skipped() {
    let dir = project_with("{}", &[("notes.md", "# notes"), ("a.spec", MESSY)]);

    let outcome = check_paths(dir.path(), &["notes.md", "a.spec"]);

    assert_eq!(outcome.checked, 1);
    assert!(outcome.failures.is_empty());
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a named directory that is a project formats that project's sources"
)]
fn a_named_project_directory_formats_its_sources() {
    let dir = project_with(
        r#"{"spec_root": "specs"}"#,
        &[("specs/a.spec", MESSY), ("fixtures/fx.spec", MESSY)],
    );

    let named = check_paths(dir.path(), &[""]);

    assert_eq!(changed_in(dir.path(), &named), ["specs/a.spec"]);
    assert_eq!(changed(dir.path()), ["specs/a.spec"]);
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a named directory that is a project formats that project's sources"
)]
fn a_walk_takes_a_nested_projects_sources() {
    let dir = project_with(
        "{}",
        &[
            ("examples/p/specforge.json", r#"{"spec_root": "spec"}"#),
            ("examples/p/spec/a.spec", MESSY),
            ("examples/p/fixtures/f.spec", MESSY),
            ("examples/loose.spec", MESSY),
        ],
    );

    let outcome = check_paths(dir.path(), &["examples"]);

    assert_eq!(
        changed_in(dir.path(), &outcome),
        ["examples/loose.spec", "examples/p/spec/a.spec"]
    );
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "Discover Format Targets: format target discovery holds — project_root_available, filesystem_accessible, all_spec_files_discovered, exclusions_applied, non_spec_skipped"
)]
fn discover_contract() {
    // project_root_available, filesystem_accessible: a project on disk.
    let dir = project_with(
        r#"{"spec_root": "spec", "exclude": ["vendor"]}"#,
        &[
            ("spec/a.spec", MESSY),
            ("spec/sub/b.spec", MESSY),
            ("spec/vendor/v.spec", MESSY),
            ("spec/c.txt", "not a spec"),
        ],
    );
    let root = dir.path();

    // all_spec_files_discovered, exclusions_applied.
    assert_eq!(
        targets(root, &[]).unwrap().files,
        [root.join("spec/a.spec"), root.join("spec/sub/b.spec")]
    );
    // non_spec_skipped: named, a non-.spec file is no target and no failure.
    let outcome = check_paths(root, &["spec/a.spec", "spec/c.txt"]);
    assert_eq!(outcome.checked, 1);
    assert!(outcome.failures.is_empty());
}

/// The paths the CI gate formats (`.github/workflows/ci.yml`:
/// `specforge format --check spec integrations/rust/spec
/// examples/todo-app examples/shop`), and the source directory each
/// one's files must lie in.
const CI_GATE: [(&str, &str); 4] = [
    ("spec", "spec"),
    ("integrations/rust/spec", "integrations/rust/spec"),
    ("examples/todo-app", "examples/todo-app/spec"),
    ("examples/shop", "examples/shop/spec"),
];

#[test]
fn the_ci_gate_paths_reach_only_their_sources() {
    let workspace = specforge_repo_root::repo_root();
    let paths: Vec<PathBuf> = CI_GATE.iter().map(|(p, _)| workspace.join(p)).collect();

    let found = targets(workspace, &paths).unwrap().files;

    let sources: Vec<PathBuf> = CI_GATE
        .iter()
        .flat_map(|(_, dir)| {
            specforge_common::SourceRoot::new(
                workspace.join(dir),
                specforge_common::Exclude::default(),
            )
            .files()
        })
        .collect();
    for target in &found {
        assert!(
            sources.contains(target),
            "{} is no source",
            target.display()
        );
    }
    assert_eq!(found.len(), sources.len());
    let outcome = run(&Request {
        root: &specforge_common::ProjectRoot::of(workspace).unwrap(),
        paths: &paths,
        mode: Mode::Check,
        caller: crate::testing::caller(),
        forms: crate::authoring::format::Forms::none(),
    });
    assert_eq!(outcome.checked, sources.len());
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a directory named like a .spec file is no target, walked or discovered"
)]
fn a_directory_named_like_a_spec_file_is_no_target() {
    let dir = project_with("{}", &[("sub/a.spec", MESSY)]);
    std::fs::create_dir_all(dir.path().join("sub/archive.spec")).unwrap();

    let outcome = run(&request(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Mode::Check,
    ));
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(outcome.checked, 1);

    // The directory walk leaves it out too.
    let walked = check_paths(dir.path(), &["sub"]);
    assert!(walked.failures.is_empty());
    assert_eq!(walked.checked, 1);
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a file several arguments reach is formatted once"
)]
fn a_file_several_arguments_reach_is_formatted_once() {
    let dir = project_with("{}", &[("a.spec", MESSY)]);
    std::fs::create_dir(dir.path().join("x")).unwrap();

    let twice = check_paths(dir.path(), &["a.spec", "x/../a.spec"]);
    assert_eq!(twice.checked, 1);
    assert_eq!(changed_in(dir.path(), &twice), ["a.spec"]);

    // `Path` equality already folds a redundant component.
    let folded = check_paths(dir.path(), &["", "a.spec"]);
    assert_eq!(folded.checked, 1);
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a named directory that is a project formats that project's sources"
)]
fn a_walk_reads_a_nested_project_directory_as_a_project() {
    let dir = project_with(
        "{}",
        &[
            ("sub/N/specforge.json", "{}"),
            ("sub/N/n.spec", MESSY),
            ("sub/N/inner/specforge.json", r#"{"exclude": ["drafts"]}"#),
            ("sub/N/inner/drafts/d.spec", MESSY),
        ],
    );

    let outcome = check_paths(dir.path(), &["sub"]);

    // N is a project, so its sources are N's own, which end at `inner`,
    // another project.
    assert_eq!(changed_in(dir.path(), &outcome), ["sub/N/n.spec"]);
}

#[specforge_test(
    behavior = "discover_format_targets",
    verify = "a project's sources end at a nested project, which a run from the project does not format"
)]
fn a_run_from_a_project_leaves_a_nested_project_alone() {
    let dir = project_with(
        "{}",
        &[
            ("a.spec", MESSY),
            ("sub/specforge.json", r#"{"exclude": ["b.spec"]}"#),
            ("sub/b.spec", MESSY),
            ("sub/c.spec", MESSY),
        ],
    );

    assert_eq!(changed(dir.path()), ["a.spec"]);
    let named = check_paths(dir.path(), &[""]);
    assert_eq!(changed_in(dir.path(), &named), ["a.spec"]);
    let nested = check_paths(dir.path(), &["sub"]);
    assert_eq!(changed_in(dir.path(), &nested), ["sub/c.spec"]);
}
