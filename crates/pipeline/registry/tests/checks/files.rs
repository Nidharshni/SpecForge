//! E016: a path a `file_reference` field names that does not exist, and the
//! one set of files the checks read.

use std::path::PathBuf;

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::{EntityRecord, RuleInput};
use specforge_registry::rules::NoVerdicts;
use specforge_registry::{Asked, MemoryDisk};
use specforge_test_macros::test as spec;

use crate::support::written::checked_on;
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

/// The spec root of the E016 tests.
const ROOT: &str = "/spec";

/// The E016s of the checks over a behavior `alpha` listing `paths` in its
/// `gherkin`, with `ROOT` as the spec root and `disk` as the file system.
fn e016(disk: &MemoryDisk, paths: &[&str]) -> Vec<Diagnostic> {
    let records =
        [EntityRecord::new("behavior", "alpha", span("main.spec")).with_list("gherkin", paths)];
    let diags = build([gherkin()]).check(
        &RuleInput {
            entities: &records,
            edges: &[],
            spec_root: std::path::Path::new(ROOT),
            disk,
        },
        &NoVerdicts,
    );
    coded_in(&diags, "E016").into_iter().cloned().collect()
}

/// A disk with `features/<name>` under the spec root for each of `names`.
fn root_with(names: &[&str]) -> MemoryDisk {
    names.iter().fold(MemoryDisk::new(), |disk, name| {
        disk.with_file(format!("{ROOT}/features/{name}"), b"Feature")
    })
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "non-existent file reference produces E016"
)]
fn a_missing_file_reference_produces_e016() {
    let errors = e016(&MemoryDisk::new(), &["features/alpha.feature"]);
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
    let errors = e016(&dir, &["features/alpha.feature"]);
    assert!(errors.is_empty(), "existing file should not produce E016");
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "multiple file references in same entity each validated independently"
)]
fn each_file_reference_of_an_entity_is_validated_independently() {
    let dir = root_with(&["alpha.feature"]);
    let errors = e016(&dir, &["features/alpha.feature", "features/beta.feature"]);
    assert_eq!(errors.len(), 1, "only the missing file should produce E016");
    assert!(errors[0].message.contains("beta.feature"));
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "relative path resolved from the spec root"
)]
fn a_relative_path_resolves_from_the_spec_root() {
    let dir = MemoryDisk::new().with_file("/spec/sub/features/alpha.feature", b"Feature: Alpha");

    let errors = e016(&dir, &["sub/features/alpha.feature"]);

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
    let errors = e016(&dir, &["features/alpa.feature"]);
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
    let errors = e016(&dir, &["features/alpha.feature"]);
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
    let errors = e016(&dir, &["features/logn.feature"]);
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
        &dir,
        &["features/exists.feature", "features/missing.feature"],
    );
    assert_eq!(errors.len(), 1, "only the missing file should produce E016");
    assert!(errors[0].message.contains("missing.feature"));
}

#[test]
fn e016_lists_the_directory_of_a_missing_file_only() {
    let disk = root_with(&["alpha.feature"]);
    let errors = e016(&disk, &["features/alpha.feature", "features/beta.feature"]);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        disk.asked(),
        [
            Asked::Exists("/spec/features/alpha.feature".into()),
            Asked::Exists("/spec/features/beta.feature".into()),
            Asked::List("/spec/features".into()),
        ]
    );
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
    verify = "the checks read only through their disk: the paths file_reference fields of the entity's kind name, the paths file_exists rules read, and the directory E016's suggestion lists"
)]
fn the_checks_read_only_through_their_disk() {
    let build = build([with_a_file_exists_rule()]);
    let records = [
        EntityRecord::new("note", "n", span("main.spec"))
            .with_list("docs", &["more/one.md", "a.md"])
            .with_field("guide", "guide.md"),
        // The same path twice is asked twice: the ledger dedups.
        EntityRecord::new("note", "m", span("main.spec")).with_list("docs", &["a.md"]),
    ];
    let disk = MemoryDisk::new();

    build.check(
        &RuleInput {
            entities: &records,
            edges: &[],
            spec_root: std::path::Path::new("/spec"),
            disk: &disk,
        },
        &NoVerdicts,
    );

    let (exists, list) = (Asked::Exists, Asked::List);
    assert_eq!(
        disk.asked(),
        [
            // E016, record by record: a missing path lists its directory.
            exists("/spec/more/one.md".into()),
            list("/spec/more".into()),
            exists("/spec/a.md".into()),
            list("/spec".into()),
            exists("/spec/a.md".into()),
            list("/spec".into()),
            // The `file_exists` rule over `guide`.
            exists("/spec/guide.md".into()),
        ]
    );
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

/// The paths the checks asked whether they exist, and their E016s.
fn reads_and_errors(records: &[EntityRecord]) -> (Vec<PathBuf>, Vec<Diagnostic>) {
    let build = build([docs_and_notes()]);
    let disk = MemoryDisk::new();
    let input = RuleInput {
        entities: records,
        edges: &[],
        spec_root: std::path::Path::new("/nonexistent/spec"),
        disk: &disk,
    };
    let diags = build.check(&input, &NoVerdicts);
    let exists = disk
        .asked()
        .into_iter()
        .filter_map(|asked| match asked {
            Asked::Exists(path) => Some(path),
            _ => None,
        })
        .collect();
    (
        exists,
        coded_in(&diags, "E016").into_iter().cloned().collect(),
    )
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "a field another kind declares as a file reference is not one on this kind"
)]
fn a_field_named_like_another_kinds_file_reference_is_not_one() {
    let (exists, errors) = reads_and_errors(&[
        EntityRecord::new("doc", "d1", span("main.spec")).with_list("paths", &["missing.md"]),
        EntityRecord::new("note", "n1", span("main.spec")).with_list("paths", &["alpha", "beta"]),
    ]);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("'missing.md'"), "{errors:?}");
    assert_eq!(
        exists,
        [std::path::Path::new("/nonexistent/spec").join("missing.md")],
        "the note's tags are not read"
    );
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "a single path on a file-reference field is checked like a one-item list"
)]
fn a_single_path_is_checked() {
    let (exists, errors) = reads_and_errors(&[
        EntityRecord::new("doc", "d1", span("main.spec")).with_field("guide", "missing.md"),
        // An empty single value names no file.
        EntityRecord::new("doc", "d2", span("main.spec")).with_field("guide", ""),
    ]);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("'missing.md'"), "{errors:?}");
    assert_eq!(
        exists,
        [std::path::Path::new("/nonexistent/spec").join("missing.md")]
    );
}

/// `note` with `guide` (a single path, a file reference) and `docs` (a list of paths, a file
/// reference), and the `file_exists` rule `F001` over `guide`.
fn notes_with_a_guide() -> ExtensionDeclaration {
    declare("@test/notes", |c| {
        c.kind("Note", |k| {
            k.keyword("note");
            k.field("guide", |f| {
                f.field_type(FieldType::String).file_reference();
            });
            k.field("docs", |f| {
                f.field_type(FieldType::StringList).file_reference();
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

/// The messages of `code` among the checks over `text`, written as one file, on `disk`.
fn messages_of(code: &str, text: &str, disk: &MemoryDisk) -> Vec<String> {
    let diagnostics = checked_on(&build([notes_with_a_guide()]), &[("t.spec", text)], disk);
    coded_in(&diagnostics, code)
        .into_iter()
        .map(|d| d.message.clone())
        .collect()
}

#[spec(
    behavior = "validate_file_reference_paths",
    verify = "a bare word on a file-reference field is a path"
)]
#[ignore = "flipped by T3"]
fn a_bare_word_on_a_file_reference_field_is_e016() {
    let text = "note bare \"B\" {\n  guide README\n}\n";

    assert_eq!(
        messages_of("E016", text, &MemoryDisk::new()),
        ["file reference 'README' in entity 'bare' does not exist"]
    );
    assert_eq!(
        messages_of("F001", text, &MemoryDisk::new()),
        ["note 'bare': missing 'README'"]
    );
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists reads no path in a block, an expression group or an empty value"
)]
#[ignore = "flipped by T3"]
fn a_file_exists_rule_reads_no_path_in_a_block_or_an_empty_value() {
    let text = "note block \"K\" {\n  guide {\n    README \"x\"\n  }\n}\n\
                note empty \"E\" {\n  guide \"\"\n}\n";
    let disk = MemoryDisk::new();

    assert!(messages_of("F001", text, &disk).is_empty());
    assert!(messages_of("E016", text, &disk).is_empty());
    let asked_exists = disk
        .asked()
        .into_iter()
        .filter(|asked| matches!(asked, Asked::Exists(_)))
        .collect::<Vec<_>>();
    assert!(asked_exists.is_empty(), "{asked_exists:?}");
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "a missing path a file_exists rule names suggests a close sibling"
)]
#[ignore = "flipped by T3"]
fn a_file_exists_rule_suggests_a_close_sibling() {
    let text = "note quoted \"Q\" {\n  guide \"NOTES\"\n}\n";
    let disk = MemoryDisk::new().with_file("/spec/NOTE", b"");

    let diagnostics = checked_on(&build([notes_with_a_guide()]), &[("t.spec", text)], &disk);

    let f001 = coded_in(&diagnostics, "F001");
    assert_eq!(f001.len(), 1, "{diagnostics:?}");
    assert_eq!(f001[0].suggestion.as_deref(), Some("did you mean 'NOTE'?"));
    let asked = disk.asked();
    let exists = asked
        .iter()
        .position(|a| *a == Asked::Exists("/spec/NOTES".into()))
        .unwrap();
    assert_eq!(asked.last(), Some(&Asked::List("/spec".into())));
    assert!(exists < asked.len() - 1);
}
