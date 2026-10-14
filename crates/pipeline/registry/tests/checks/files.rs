//! E016: a path a `file_reference` field names that does not exist, and the
//! one set of files the checks read.

use std::path::PathBuf;

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::{EntityRecord, RuleInput};
use specforge_registry::rules::NoVerdicts;
use specforge_test_macros::test as spec;
use tempfile::TempDir;

use crate::support::{build, coded_in, declare, span};

/// `behavior`, with `gherkin` (a list of paths, a file reference) and
/// `notes` (a list of strings that are no file reference).
fn gherkin() -> ExtensionDeclaration {
    declare("@test/gherkin", |c| {
        c.kind("Behavior", |k| {
            k.keyword("behavior");
            k.field("gherkin", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
            k.field("notes", |f| {
                f.field_type(FieldType::StringList);
            });
        });
    })
}

/// The E016s of the checks over a behavior `alpha` listing `paths` in its
/// `gherkin`, with `root` as the spec root.
fn e016(root: &std::path::Path, paths: &[&str]) -> Vec<Diagnostic> {
    let records =
        [EntityRecord::new("behavior", "alpha", span("main.spec")).with_list("gherkin", paths)];
    let diags = build([gherkin()]).check(
        &RuleInput {
            entities: &records,
            edges: &[],
            spec_root: root,
        },
        &NoVerdicts,
    );
    coded_in(&diags, "E016").into_iter().cloned().collect()
}

/// A spec root with `features/<name>` for each of `names`.
fn root_with(names: &[&str]) -> TempDir {
    let dir = TempDir::new().unwrap();
    let features = dir.path().join("features");
    std::fs::create_dir_all(&features).unwrap();
    for name in names {
        std::fs::write(features.join(name), "Feature").unwrap();
    }
    dir
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "non-existent file reference produces E016"
)]
fn a_missing_file_reference_produces_e016() {
    let errors = e016(
        std::path::Path::new("/nonexistent/project"),
        &["features/alpha.feature"],
    );
    assert_eq!(errors.len(), 1, "missing file should produce E016");
    assert!(errors[0].message.contains("alpha.feature"));
    assert!(errors[0].span.is_some());
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "existing file reference passes silently"
)]
fn an_existing_file_reference_passes() {
    let dir = root_with(&["alpha.feature"]);
    let errors = e016(dir.path(), &["features/alpha.feature"]);
    assert!(errors.is_empty(), "existing file should not produce E016");
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "multiple file references in same entity each validated independently"
)]
fn each_file_reference_of_an_entity_is_validated_independently() {
    let dir = root_with(&["alpha.feature"]);
    let errors = e016(
        dir.path(),
        &["features/alpha.feature", "features/beta.feature"],
    );
    assert_eq!(errors.len(), 1, "only the missing file should produce E016");
    assert!(errors[0].message.contains("beta.feature"));
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "relative path resolved from the spec root"
)]
fn a_relative_path_resolves_from_the_spec_root() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("sub").join("features");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("alpha.feature"), "Feature: Alpha").unwrap();

    let errors = e016(dir.path(), &["sub/features/alpha.feature"]);

    assert!(
        errors.is_empty(),
        "a path relative to the spec root should resolve, got: {errors:?}"
    );
}

#[spec(
    behavior = "provide_did_you_mean_suggestions",
    verify = "close match produces suggestion"
)]
fn e016_suggests_a_similar_filename() {
    let dir = root_with(&["alpha.feature"]);
    // A typo: "alpa.feature" for "alpha.feature".
    let errors = e016(dir.path(), &["features/alpa.feature"]);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .suggestion
            .as_ref()
            .is_some_and(|s| s.contains("alpha.feature")),
        "E016 should suggest 'alpha.feature', got: {:?}",
        errors[0].suggestion
    );
}

#[spec(
    behavior = "provide_did_you_mean_suggestions",
    verify = "distant match produces no suggestion"
)]
fn e016_suggests_nothing_when_no_file_is_similar() {
    let dir = root_with(&["zebra.feature"]);
    let errors = e016(dir.path(), &["features/alpha.feature"]);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].suggestion.is_none(),
        "should not suggest an unrelated file, got: {:?}",
        errors[0].suggestion
    );
}

#[spec(
    behavior = "provide_did_you_mean_suggestions",
    verify = "suggestion appears in help text"
)]
fn the_suggestion_appears_in_the_rendered_help() {
    let dir = root_with(&["login.feature"]);
    // A typo: "logn.feature" for "login.feature".
    let errors = e016(dir.path(), &["features/logn.feature"]);
    assert_eq!(errors.len(), 1);

    let source = "behavior alpha \"A\" {\n  gherkin [\"features/logn.feature\"]\n}\n";
    let sources = std::collections::HashMap::from([("main.spec".to_string(), source.to_string())]);
    let rendered = specforge_common::render_diagnostics(&errors, &sources, false);
    let help: Vec<&str> = rendered
        .lines()
        .filter(|line| line.contains("Help:"))
        .collect();
    assert_eq!(help.len(), 1, "{rendered}");
    assert!(
        help[0].ends_with("Help: did you mean 'features/login.feature'?"),
        "{rendered}"
    );
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "Validate File Reference Paths: file reference validation holds — graph_built_fired, filesystem_available, missing_files_diagnosed, existing_files_pass"
)]
fn the_file_reference_contract_holds() {
    let dir = root_with(&["exists.feature"]);
    // missing_files_diagnosed and existing_files_pass.
    let errors = e016(
        dir.path(),
        &["features/exists.feature", "features/missing.feature"],
    );
    assert_eq!(errors.len(), 1, "only the missing file should produce E016");
    assert!(errors[0].message.contains("missing.feature"));
}

/// A kind with `file_reference` fields and a `file_exists` rule over
/// another field.
fn with_a_file_exists_rule() -> ExtensionDeclaration {
    declare("@test/files", |c| {
        c.kind("Note", |k| {
            k.keyword("note");
            k.field("docs", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
            k.field("guide", |f| {
                f.field_type(FieldType::String);
            });
        });
        c.rule("F001", |r| {
            r.check(CheckKind::FileExists)
                .severity(ValidationSeverity::Warning)
                .target_kind("note")
                .field("guide")
                .message_template("note '{id}': missing '{value}'");
        });
    })
}

#[spec(
    behavior = "check_entities_in_one_order",
    verify = "the files the checks read are those file_reference fields name and those file_exists rules read"
)]
fn the_files_the_checks_read_are_one_set() {
    let build = build([with_a_file_exists_rule()]);
    let records = [
        EntityRecord::new("note", "n", span("main.spec"))
            .with_list("docs", &["more/one.md", "a.md"])
            .with_field("guide", "guide.md"),
        // The same path twice is listed once.
        EntityRecord::new("note", "m", span("main.spec")).with_list("docs", &["a.md"]),
    ];
    let root = std::path::Path::new("/spec");

    let files = build.files(&RuleInput {
        entities: &records,
        edges: &[],
        spec_root: root,
    });

    let expected: Vec<PathBuf> = ["a.md", "guide.md", "more/one.md"]
        .iter()
        .map(|p| root.join(p))
        .collect();
    assert_eq!(files, expected);
}

/// `doc` has file-reference fields (`paths`, a list; `guide`, a single
/// path); `note` has a `paths` that is no file reference.
fn docs_and_notes() -> ExtensionDeclaration {
    declare("@test/docs", |c| {
        c.kind("Doc", |k| {
            k.keyword("doc");
            k.field("paths", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
            k.field("guide", |f| {
                f.field_type(FieldType::String).file_reference();
            });
        });
        c.kind("Note", |k| {
            k.keyword("note");
            k.field("paths", |f| {
                f.field_type(FieldType::StringList);
            });
        });
    })
}

fn files_and_errors(records: &[EntityRecord]) -> (Vec<PathBuf>, Vec<Diagnostic>) {
    let build = build([docs_and_notes()]);
    let root = std::path::Path::new("/nonexistent/spec");
    let input = RuleInput {
        entities: records,
        edges: &[],
        spec_root: root,
    };
    let diags = build.check(&input, &NoVerdicts);
    (
        build.files(&input),
        coded_in(&diags, "E016").into_iter().cloned().collect(),
    )
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "a field another kind declares as a file reference is not one on this kind"
)]
fn a_field_named_like_another_kinds_file_reference_is_not_one() {
    let (files, errors) = files_and_errors(&[
        EntityRecord::new("doc", "d1", span("main.spec")).with_list("paths", &["missing.md"]),
        EntityRecord::new("note", "n1", span("main.spec")).with_list("paths", &["alpha", "beta"]),
    ]);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("'missing.md'"), "{errors:?}");
    assert_eq!(
        files,
        [std::path::Path::new("/nonexistent/spec").join("missing.md")],
        "the note's tags are not watched"
    );
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "a single path on a file-reference field is checked like a one-item list"
)]
fn a_single_path_is_checked() {
    let (files, errors) = files_and_errors(&[
        EntityRecord::new("doc", "d1", span("main.spec")).with_field("guide", "missing.md"),
        // An empty single value names no file.
        EntityRecord::new("doc", "d2", span("main.spec")).with_field("guide", ""),
    ]);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("'missing.md'"), "{errors:?}");
    assert_eq!(
        files,
        [std::path::Path::new("/nonexistent/spec").join("missing.md")]
    );
}
