use super::*;
use crate::repo::{OUTSIDE_WORKSPACE, this_repo};

fn check(name: &str) -> &'static Check {
    CHECKS
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name}"))
}

fn selection(name: &str, mode: Mode, paths: &[&str]) -> Selection {
    check(name)
        .selection(mode, &this_repo(), &ChangeSet::of(paths.iter().copied()))
        .unwrap()
}

fn only(names: &[&str]) -> Selection {
    Selection::Only(names.iter().map(|n| n.to_string()).collect())
}

#[test]
fn every_check_has_a_unique_name_a_fix_and_known_after_names() {
    let mut seen = BTreeSet::new();
    for check in CHECKS {
        assert!(seen.insert(check.name), "duplicate check {}", check.name);
        assert!(
            check
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-'),
            "{} is not kebab-case",
            check.name
        );
        assert!(
            !check.fix.is_empty() && !check.proves.is_empty(),
            "{} lacks a fix or a proof",
            check.name
        );
    }
    for check in CHECKS {
        for after in check.after {
            let position = CHECKS.iter().position(|c| c.name == *after);
            let own = CHECKS.iter().position(|c| c.name == check.name);
            assert!(
                position < own && position.is_some(),
                "{} runs after {after}, which does not precede it",
                check.name
            );
        }
    }
}

#[test]
fn ci_and_full_select_the_same_checks() {
    let repo = this_repo();
    let changes = ChangeSet::of(["README.md"]);
    for check in CHECKS {
        assert_eq!(
            check.selection(Mode::Ci, &repo, &changes).unwrap(),
            check.selection(Mode::Full, &repo, &changes).unwrap(),
            "{}",
            check.name
        );
    }
}

#[test]
fn the_hook_reaches_no_check_that_compiles_and_not_the_builtins() {
    let repo = this_repo();
    let everything = ChangeSet::of(["crates/specforge-lsp/src/lib.rs", "a.spec", "Cargo.toml"]);
    for check in CHECKS {
        let selected = check.selection(Mode::Hook, &repo, &everything).unwrap() != Selection::Skip;
        if selected {
            assert!(
                !check.compiles,
                "the hook reaches {}, which compiles the workspace",
                check.name
            );
            assert!(
                check.reach == Reach::Hook,
                "the hook reaches {}",
                check.name
            );
        }
    }
    assert_eq!(
        selection("builtins", Mode::Hook, &["extensions/product/src/lib.rs"]),
        Selection::Skip
    );
}

#[test]
fn full_reaches_every_check_but_builtins_verify() {
    let repo = this_repo();
    let nothing = ChangeSet::default();
    for check in CHECKS {
        let selected = check.selection(Mode::Full, &repo, &nothing).unwrap();
        let expected = if check.name == "builtins-verify" {
            Selection::Skip
        } else {
            Selection::Everything
        };
        assert_eq!(selected, expected, "{}", check.name);
    }
}

#[test]
fn bootstrap_reaches_only_builtins_verify() {
    let repo = this_repo();
    let nothing = ChangeSet::default();
    for check in CHECKS {
        let selected =
            check.selection(Mode::Bootstrap, &repo, &nothing).unwrap() != Selection::Skip;
        assert_eq!(selected, check.name == "builtins-verify", "{}", check.name);
    }
}

#[test]
fn an_unformatted_guest_is_judged_by_the_hook() {
    // §3.2: the hook ran `cargo fmt --all`, which skips every crate outside the workspace.
    assert_eq!(
        selection("rustfmt", Mode::Hook, &["extensions/governance/src/lib.rs"]),
        Selection::Everything
    );
    assert_eq!(
        selection("rustfmt", Mode::Hook, &["docs/guide.md"]),
        Selection::Skip
    );
    let manifests = OUTSIDE_WORKSPACE
        .iter()
        .flat_map(|pattern| manifests_matching(pattern))
        .count();
    assert_eq!(manifests, 13, "nine guests and four fixtures");
}

fn manifests_matching(pattern: &str) -> Vec<String> {
    let root = this_repo().root;
    let (parent, _) = pattern.split_once("/*/").unwrap();
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root.join(parent)).unwrap().flatten() {
        let relative = format!(
            "{parent}/{}/Cargo.toml",
            entry.file_name().to_string_lossy()
        );
        if root.join(&relative).is_file() {
            found.push(relative);
        }
    }
    found
}

#[test]
fn a_staged_starter_template_is_not_format_checked() {
    // §3.5: a template a blob embeds, and a parser fixture, are formatted or not by their tests' needs.
    for path in [
        "extensions/software/src/starter.spec",
        "crates/specforge-parser/tests/fixtures/lexemes.spec",
    ] {
        assert_eq!(
            selection("spec-format", Mode::Hook, &[path]),
            Selection::Skip,
            "{path}"
        );
    }
    assert_eq!(
        selection(
            "spec-format",
            Mode::Hook,
            &["crates/specforge-cli/tests/fixtures/parity/clean/spec/a.spec"]
        ),
        Selection::Skip
    );
    assert_eq!(
        selection(
            "spec-format",
            Mode::Hook,
            &["spec/governance/invariants.spec"]
        ),
        only(&["spec/governance/invariants.spec"])
    );
    assert_eq!(
        selection(
            "spec-format",
            Mode::Hook,
            &[
                "examples/shop/spec/a.spec",
                "examples/todo-app/spec/b/c.spec"
            ]
        ),
        only(&[
            "examples/shop/spec/a.spec",
            "examples/todo-app/spec/b/c.spec"
        ])
    );
    assert_eq!(
        selection(
            "spec-format",
            Mode::Hook,
            &["crates/specforge-formatter/src/lib.rs"]
        ),
        Selection::Everything
    );
}

#[test]
fn a_grammar_edit_is_judged_by_the_hook() {
    assert_eq!(
        selection(
            "grammar",
            Mode::Hook,
            &["crates/tree-sitter-specforge/grammar.js"]
        ),
        Selection::Everything
    );
    assert_eq!(
        selection(
            "grammar",
            Mode::Hook,
            &["crates/tree-sitter-specforge/src/parser.c"]
        ),
        Selection::Everything
    );
    assert_eq!(
        selection("grammar", Mode::Hook, &["crates/specforge-lsp/src/lib.rs"]),
        Selection::Skip
    );
}

#[test]
fn quick_runs_the_guests_and_the_specs_when_a_rule_crate_changes() {
    // gate.sh:227 ran `specforge check` only for the emitter, and no guest for an SDK edit.
    let path = "crates/specforge-registry/src/checks/values.rs";
    assert_eq!(
        selection("guests", Mode::Quick, &[path]),
        Selection::Everything
    );
    assert_ne!(selection("specs", Mode::Quick, &[path]), Selection::Skip);
    assert_eq!(
        selection(
            "guests",
            Mode::Quick,
            &["crates/specforge-extension-sdk/src/lib.rs"]
        ),
        Selection::Everything
    );
    assert_eq!(
        selection("specs", Mode::Quick, &["docs/guide.md"]),
        Selection::Skip
    );
}

#[test]
fn a_test_file_or_a_verify_attribute_or_a_spec_runs_links() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("crates/a/src")).unwrap();
    std::fs::write(
        dir.path().join("crates/a/src/linked.rs"),
        "#[test(verify   = \"x\")]\nfn t() {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("crates/a/src/plain.rs"),
        "fn verified() {}\nlet verify_all = 1;\n",
    )
    .unwrap();
    let repo = Repo {
        root: dir.path().to_path_buf(),
        target_dir: dir.path().join("target"),
        members: vec![],
    };
    let run = |paths: &[&str]| {
        check("links")
            .selection(Mode::Hook, &repo, &ChangeSet::of(paths.iter().copied()))
            .unwrap()
    };
    assert_eq!(run(&["crates/a/src/linked.rs"]), Selection::Everything);
    assert_eq!(run(&["crates/a/src/plain.rs"]), Selection::Skip);
    assert_eq!(run(&["crates/a/tests/t.rs"]), Selection::Everything);
    assert_eq!(run(&["tests/t.rs"]), Selection::Everything);
    assert_eq!(run(&["spec/glossary.spec"]), Selection::Everything);
    assert_eq!(run(&["README.md"]), Selection::Skip);
}

#[test]
fn packages_map_changed_paths_to_members_and_manifests_to_everything() {
    assert_eq!(
        selection(
            "tests",
            Mode::Quick,
            &[
                "crates/specforge-lsp/src/lib.rs",
                "crates/specforge-lsp/tests/a.rs",
                "xtask/src/x.rs"
            ]
        ),
        only(&["specforge-lsp", "xtask"])
    );
    assert_eq!(
        selection("build", Mode::Quick, &["Cargo.toml"]),
        Selection::Everything
    );
    assert_eq!(
        selection("build", Mode::Quick, &["Cargo.lock"]),
        Selection::Everything
    );
    assert_eq!(
        selection("tests", Mode::Quick, &[".config/nextest.toml"]),
        Selection::Everything
    );
    assert_eq!(
        selection(
            "build",
            Mode::Quick,
            &["extensions/governance/src/lib.rs", "README.md"]
        ),
        Selection::Skip
    );
}

#[test]
fn doctests_run_for_the_changed_members_that_hold_one() {
    assert_eq!(
        selection(
            "doctests",
            Mode::Quick,
            &["crates/specforge-diagnostics/src/lib.rs"]
        ),
        only(&["specforge-diagnostics"])
    );
    assert_eq!(
        selection(
            "doctests",
            Mode::Quick,
            &["crates/specforge-lsp/src/lib.rs"]
        ),
        Selection::Skip
    );
}

#[test]
fn quick_runs_the_builtins_check_when_a_recorded_input_changes() {
    // §3.1: quick passed all four of these edits while `build-blobs --check` failed on them.
    for path in [
        "crates/specforge-coverage/src/lib.rs",
        "crates/specforge-protocol-types/src/lib.rs",
        "Cargo.toml",
        "crates/specforge-extension-sdk/src/lib.rs",
    ] {
        assert_ne!(
            selection("builtins", Mode::Quick, &[path]),
            Selection::Skip,
            "{path}"
        );
    }
    assert_eq!(
        selection(
            "builtins",
            Mode::Quick,
            &["crates/specforge-lsp/src/lib.rs"]
        ),
        Selection::Skip
    );
    assert_eq!(
        selection(
            "builtins",
            Mode::Quick,
            &["crates/specforge-coverage/src/lib.rs"]
        ),
        only(&[
            "extensions/formal",
            "extensions/product",
            "extensions/testing"
        ])
    );
}

#[test]
fn the_list_is_pinned() {
    insta::assert_snapshot!(render_list());
}

#[test]
fn published_schemas_run_when_a_writer_or_a_declaration_changes() {
    for path in [
        "crates/specforge-emitter/src/publish.rs",
        "extensions/formal/src/declaration.rs",
        "spec/behaviors/output-schema.spec",
    ] {
        assert_eq!(
            selection("published-schemas", Mode::Quick, &[path]),
            Selection::Everything,
            "{path}"
        );
    }
    assert_eq!(
        selection("published-schemas", Mode::Quick, &["README.md"]),
        Selection::Skip
    );
    assert_eq!(
        selection("published-schemas", Mode::Hook, &["spec/x.spec"]),
        Selection::Skip,
        "a quick check is not a hook one"
    );
}
