//! The structural checks over a compiled project's entities (architecture
//! plan 04, T0): the order they report in, the gate that switches the kind
//! checks off, which fields count as file references, and which rules fire.
//! Each pin that encodes a bug says which ticket flips it.

use std::fs;

use serde_json::json;
use specforge_common::Diagnostic;
use specforge_extension_sdk::PeerDependency;
use specforge_extension_sdk::prelude::*;
use specforge_project::CompiledProject;
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

/// A project loading `extensions`, with `a.spec` as given.
fn project(extensions: &[&str], spec: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let config = json!({ "name": "p", "version": "0.1.0", "extensions": extensions });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    fs::write(dir.path().join("a.spec"), spec).unwrap();
    dir
}

/// The compile's diagnostics as `[code, message]` pairs, keeping only
/// `codes`, in the order the compile reported them.
fn reported(runtime: InProcessRuntime, dir: &TempDir, codes: &[&str]) -> Vec<[String; 2]> {
    let diagnostics: Vec<Diagnostic> = CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(std::sync::Arc::new(runtime)),
    )
    .diagnostics();
    diagnostics
        .into_iter()
        .filter(|d| codes.contains(&d.code.as_str()))
        .map(|d| [d.code.clone(), d.message.clone()])
        .collect()
}

fn codes_of(reported: &[[String; 2]]) -> Vec<&str> {
    reported.iter().map(|[code, _]| code.as_str()).collect()
}

/// `gadget` (an integer, an enum, a reference list to gadgets, a list of
/// files), `widget`, and a rule that fires on a widget linking nothing.
fn shapes() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/shapes", "1.0.0"));
        c.kind("gadget", |k| {
            k.keyword("gadget");
            k.field("size", |f| {
                f.field_type(FieldType::Integer);
            });
            k.field("mode", |f| {
                f.field_type(FieldType::Enum).enum_values(&["fast", "slow"]);
            });
            k.field("parts", |f| {
                f.field_type(FieldType::ReferenceList).target_kind("gadget");
            });
            k.field("docs", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
        });
        c.kind("widget", |k| {
            k.keyword("widget");
        });
        c.rule("W900", |r| {
            r.check(CheckKind::NoOutgoingEdges)
                .severity(ValidationSeverity::Warning)
                .target_kind("widget")
                .message_template("{kind} '{id}' links nothing");
        });
        c
    })
}

/// One entity per check, in an order that is not the checks' order, so a
/// compile that reorders them shows.
const ONE_OF_EACH: &str = "\
ref gh.issue:1 \"Unreferenced\"
gadget g_docs { docs [\"missing.md\"] }
wibble wb_one { }
gadget gadget { }
gadget x { }
gadget g_bogus { bogus \"1\" }
widget w_one { }
gadget g_parts { parts [w_one] }
gadget g_size { size \"big\" }
";

const ORDER: [&str; 9] = [
    "W012", "E016", "E024", "E013", "E014", "W020", "E022", "E061", "W900",
];

/// Every structural check reports, then the extension's rule: the order is
/// W012, E016, E024, E013, E014, W020, E022, E061, then the rules. It must
/// stay byte-identical while the checks move (plan 04, T0..T12).
#[specforge_test(
    behavior = "check_entities_in_one_order",
    verify = "a compile reports the structural checks in the order the registry build runs them"
)]
fn structural_checks_report_in_one_order() {
    let dir = project(&["@test/shapes"], ONE_OF_EACH);

    let found = reported(shapes(), &dir, &ORDER);

    assert_eq!(codes_of(&found), ORDER, "{found:#?}");
    insta::assert_json_snapshot!("one_order", found);
}

/// An extension that declares a rule and no kind.
fn kindless() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/kindless", "1.0.0"));
        c.rule("W902", |r| {
            r.check(CheckKind::NoIncomingEdges)
                .severity(ValidationSeverity::Warning)
                .target_kind("ref")
                .message_template("{kind} '{id}' is not referenced");
        });
        c
    })
}

/// Extensions that load but declare no kind switch the kind, field and
/// identifier checks off; one W151 says so, naming the entities nobody
/// checks (no E024 for `wibble`, no E014 for the short ID, no I002).
#[test]
fn kindless_extensions_leave_entities_unchecked() {
    let dir = project(&["@test/kindless"], "wibble w { }\nthing ab { }\n");

    let found = reported(
        kindless(),
        &dir,
        &[
            "E024", "E013", "E014", "W020", "E022", "E061", "W151", "I002",
        ],
    );

    assert_eq!(codes_of(&found), ["W151"], "{found:#?}");
    assert_eq!(
        found[0][1],
        "the loaded extensions declare no entity kind: 2 entities (kinds: thing, wibble) are not checked against kinds, fields or identifiers"
    );
}

/// A rule targeting a kind `@test/absent` would declare, and nobody loaded.
fn ghost_rules() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut meta = ExtensionMeta::new("@test/ghost-rules", "1.0.0");
        meta.peer_dependencies = vec![PeerDependency {
            name: "@test/absent".to_string(),
            version: "^1.0".to_string(),
            optional: true,
        }];
        let mut c = ContributionsBuilder::new(meta);
        // A declared kind, so the kind checks run (E024 for `ghost`).
        c.kind("anchor", |k| {
            k.keyword("anchor");
        });
        c.rule("W901", |r| {
            r.check(CheckKind::NoIncomingEdges)
                .severity(ValidationSeverity::Warning)
                .target_kind("ghost")
                .message_template("{kind} '{id}' is not referenced");
        });
        c
    })
}

/// ADR 0020 D5: a rule for an undeclared kind is inert. It does not fire on
/// that keyword's entities, which are E024.
#[test]
fn a_rule_is_inert_for_an_undeclared_target_kind() {
    let dir = project(&["@test/ghost-rules"], "ghost g1 { }\n");

    let found = reported(ghost_rules(), &dir, &["E024", "W901"]);

    assert_eq!(codes_of(&found), ["E024"], "{found:#?}");
}

/// `doc` has file-reference fields (`paths`, `guide`); `note` has a field
/// named `paths` that is not one.
fn docs() -> InProcessRuntime {
    InProcessRuntime::new().with(|| {
        let mut c = ContributionsBuilder::new(ExtensionMeta::new("@test/docs", "1.0.0"));
        c.kind("doc", |k| {
            k.keyword("doc");
            k.field("paths", |f| {
                f.field_type(FieldType::StringList).file_reference();
            });
            k.field("guide", |f| {
                f.field_type(FieldType::String).file_reference();
            });
        });
        c.kind("note", |k| {
            k.keyword("note");
            k.field("paths", |f| {
                f.field_type(FieldType::StringList);
            });
        });
        c
    })
}

/// `note.paths` is no file reference: a field of that name is one on `doc`
/// only (a file reference is one on the kinds that declare it).
#[specforge_test(
    behavior = "validate_file_reference_paths",
    verify = "a field another kind declares as a file reference is not one on this kind"
)]
fn a_field_named_like_another_kinds_file_reference_is_not_checked() {
    let dir = project(&["@test/docs"], "note n1 { paths [\"alpha\"] }\n");

    let found = reported(docs(), &dir, &["E016"]);

    assert!(found.is_empty(), "{found:#?}");
}

/// A single path on a `string` file-reference field is checked like a
/// one-item list.
#[specforge_test(
    behavior = "validate_file_reference_paths",
    verify = "a single path on a file-reference field is checked like a one-item list"
)]
fn a_single_path_on_a_file_reference_field_is_checked() {
    let dir = project(&["@test/docs"], "doc d1 { guide \"missing.md\" }\n");

    let found = reported(docs(), &dir, &["E016"]);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0][1].contains("'missing.md'"), "{found:#?}");
}
