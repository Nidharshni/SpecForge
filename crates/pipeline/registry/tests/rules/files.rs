//! E016, the host's `file_exists` rule for each `file_reference` field, and every `file_exists`
//! rule's reading of the paths a field names (ADR 0118), run through `RegistryBuild::check`.

use std::path::{Path, PathBuf};

use specforge_common::Diagnostic;
use specforge_extension_sdk::prelude::*;
use specforge_protocol_types::ExtensionDeclaration;
use specforge_registry::entity::{EntityRecord, RuleInput};
use specforge_registry::rules::NoVerdicts;
use specforge_registry::{Asked, MemoryDisk};
use specforge_test_macros::test as spec;

use super::{entity, messages, one, rule};
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
    verify = "the checks read only through their disk: the paths file_exists rules read, E016's among them, and the directory a missing path's suggestion lists"
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
            // The extensions' `file_exists` rules by code, then the host's E016 rules by kind
            // and field: F001 over `guide`, a missing path lists its directory.
            exists("/spec/guide.md".into()),
            list("/spec".into()),
            // E016 over `note.docs`, entity by entity.
            exists("/spec/more/one.md".into()),
            list("/spec/more".into()),
            exists("/spec/a.md".into()),
            list("/spec".into()),
            exists("/spec/a.md".into()),
            list("/spec".into()),
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

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists reports missing file-reference field targets"
)]
fn file_exists_reports_missing_file_reference_field_targets() {
    let mut declared = rule("E101", "file_exists");
    declared.severity = ValidationSeverity::Error;
    declared.message_template = "{kind} '{id}' references missing file".to_string();
    declared.field = Some("gherkin".to_string());
    let built = one(declared);
    let disk = MemoryDisk::new().with_file("/spec/features/login.feature", b"");
    let naming = |id: &str, path: &str| entity(id, "behavior", 1, 0).with_field("gherkin", path);
    let entities = [
        naming("b1", "features/login.feature"),
        naming("b2", "features/logout.feature"),
        naming("b3", "/nonexistent/file.feature"),
        naming("b4", "/spec/features/login.feature"),
    ];

    // Relative paths are the spec root's; absolute ones are checked as
    // written.
    let input = RuleInput {
        entities: &entities,
        edges: &[],
        spec_root: Path::new("/spec"),
        disk: &disk,
    };
    let diagnostics = built.rules.check(&input, &NoVerdicts);
    assert_eq!(
        messages(&diagnostics),
        [
            "behavior 'b2' references missing file",
            "behavior 'b3' references missing file",
        ]
    );
    assert!(diagnostics.iter().all(|d| d.code == "E101"));
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists resolves a relative path against the spec root, never the working directory"
)]
fn file_exists_resolves_against_the_spec_root_not_the_working_directory() {
    let mut declared = rule("E101", "file_exists");
    declared.field = Some("doc".to_string());
    declared.message_template = "{id}: missing '{value}'".to_string();
    let built = one(declared);
    let entities = [entity("b1", "behavior", 0, 0).with_field("doc", "Cargo.toml")];
    let beside = MemoryDisk::new().with_file("Cargo.toml", b"");
    let input = RuleInput {
        entities: &entities,
        edges: &[],
        spec_root: Path::new("/spec"),
        disk: &beside,
    };
    assert_eq!(
        messages(&built.rules.check(&input, &NoVerdicts)),
        ["b1: missing 'Cargo.toml'"]
    );
    // The rule asks for the spec root's file, never the working directory's, and lists the spec
    // root for the missing file's close sibling.
    assert_eq!(
        beside.asked(),
        [
            Asked::Exists("/spec/Cargo.toml".into()),
            Asked::List("/spec".into())
        ]
    );
}

#[spec(
    behavior = "execute_validation_pattern",
    verify = "file_exists checks each item of a list field as its own path"
)]
fn file_exists_checks_each_item_of_a_list_field() {
    let mut declared = rule("E102", "file_exists");
    declared.field = Some("docs".to_string());
    declared.message_template = "{id}: missing '{value}'".to_string();
    let built = one(declared);
    let root = Path::new("/spec");
    let disk = MemoryDisk::new().with_file("/spec/a.md", b"");
    let entities = [
        entity("b1", "behavior", 0, 0).with_list("docs", &["a.md", "b.md"]),
        // An empty list names no path.
        entity("b2", "behavior", 0, 0).with_list("docs", &[]),
        // A scalar is one path, as written.
        entity("b3", "behavior", 0, 0).with_field("docs", "a.md, b.md"),
    ];
    let input = RuleInput {
        entities: &entities,
        edges: &[],
        spec_root: root,
        disk: &disk,
    };

    let diagnostics = built.rules.check(&input, &NoVerdicts);

    assert_eq!(
        messages(&diagnostics),
        ["b1: missing 'b.md'", "b3: missing 'a.md, b.md'"]
    );
    // The rule asks about every item against the spec root, in entity order, and lists the
    // directory of each missing one.
    assert_eq!(
        disk.asked(),
        [
            Asked::Exists(root.join("a.md")),
            Asked::Exists(root.join("b.md")),
            Asked::List(root.to_path_buf()),
            Asked::Exists(root.join("a.md, b.md")),
            Asked::List(root.to_path_buf()),
        ]
    );
}
