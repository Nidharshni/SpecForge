use specforge_common::{Diagnostic, Severity};
use specforge_parser::{SpecFile, parse};
use specforge_resolver::{import_target, resolve_imports};
use specforge_test_macros::test as specforge_test;
use std::collections::BTreeSet;

/// The import diagnostics of `files` (key, text), parsed as a compile parses
/// them, `unreadable` held but unread.
fn resolve(files: &[(&str, &str)], unreadable: &[&str]) -> Vec<Diagnostic> {
    let parsed: Vec<(&str, SpecFile)> = files.iter().map(|(k, t)| (*k, parse(t, k))).collect();
    let files: Vec<(&str, &SpecFile)> = parsed.iter().map(|(k, f)| (*k, f)).collect();
    resolve_imports(&files, unreadable)
}

/// A project's sources, as (key, text).
fn setup_project<'a>(files: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    files.to_vec()
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn resolve_use_import_to_file() {
    let dir = setup_project(&[
        ("types.spec", r#"behavior alpha "A" { contract "first" }"#),
        (
            "main.spec",
            "use \"types\"\nbehavior beta \"B\" {\n  invariants [alpha]\n}",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "unexpected errors: {:?}",
        diagnostics
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "missing import file produces E025"
)]
fn missing_import_produces_e025() {
    let dir = setup_project(&[("main.spec", "use \"nonexistent\"\nbehavior foo \"F\" { }")]);

    let diagnostics = resolve(&dir, &[]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(errors.len(), 1, "should produce E025 for missing import");
}

#[specforge_test(
    behavior = "detect_import_cycles",
    verify = "detect direct cycle between two files"
)]
fn detect_direct_import_cycle() {
    let dir = setup_project(&[
        ("a.spec", "use \"b\"\nbehavior alpha \"A\" { }"),
        ("b.spec", "use \"a\"\nbehavior beta \"B\" { }"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let cycle_warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W113").collect();
    assert!(
        !cycle_warnings.is_empty(),
        "should detect import cycle with W113"
    );
    assert!(
        cycle_warnings
            .iter()
            .all(|d| d.severity == Severity::Warning),
        "import cycles should be warnings, not errors"
    );
}

#[specforge_test(
    behavior = "detect_import_cycles",
    verify = "detect transitive cycle across three files"
)]
fn detect_transitive_import_cycle() {
    let dir = setup_project(&[
        ("a.spec", "use \"b\"\nbehavior alpha \"A\" { }"),
        ("b.spec", "use \"c\"\nbehavior beta \"B\" { }"),
        ("c.spec", "use \"a\"\nbehavior gamma \"G\" { }"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let cycle_warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W113").collect();
    assert!(
        !cycle_warnings.is_empty(),
        "should detect transitive cycle with W113"
    );
}

#[specforge_test(
    behavior = "detect_import_cycles",
    verify = "non-cyclic files still process when a cycle exists"
)]
fn non_cyclic_files_still_resolve_when_cycle_exists() {
    let dir = setup_project(&[
        ("a.spec", "use \"b\"\nbehavior alpha \"A\" { }"),
        ("b.spec", "use \"a\"\nbehavior beta \"B\" { }"),
        ("clean.spec", "behavior gamma \"G\" { status \"ok\" }"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    // The cycle is reported, and the clean file takes no part in it.
    let w113: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "W113")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(w113, ["circular import detected: a.spec -> b.spec"]);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "non-cyclic file should still be resolved: {diagnostics:?}"
    );
}

// --- nested directory imports ---

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "imports across nested directories resolve correctly"
)]
fn imports_across_nested_directories_resolve_correctly() {
    let dir = setup_project(&[
        (
            "sub/types.spec",
            r#"behavior alpha "A" { contract "first" }"#,
        ),
        (
            "main.spec",
            "use \"sub/types\"\nbehavior beta \"B\" {\n  invariants [alpha]\n}",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "nested import should resolve without errors: {:?}",
        diagnostics
    );
}

// --- link_entity_references ---

// --- 5-step path resolution cascade ---

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn resolve_relative_dot_slash() {
    let dir = setup_project(&[
        (
            "sub/helper.spec",
            r#"behavior helper "H" { contract "help" }"#,
        ),
        (
            "sub/main.spec",
            "use \"./helper\"\nbehavior user \"U\" { invariants [helper] }",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "relative ./helper should resolve without errors: {:?}",
        diagnostics
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn resolve_relative_dot_dot_slash() {
    let dir = setup_project(&[
        (
            "shared.spec",
            r#"behavior shared "S" { contract "shared" }"#,
        ),
        (
            "sub/main.spec",
            "use \"../shared\"\nbehavior user \"U\" { invariants [shared] }",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "relative ../shared should resolve without errors: {:?}",
        diagnostics
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "relative import traversing above spec_root is rejected"
)]
fn resolve_relative_escaping_spec_root() {
    let dir = setup_project(&[(
        "sub/main.spec",
        "use \"../../escape\"\nbehavior user \"U\" { }",
    )]);

    let diagnostics = resolve(&dir, &[]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(
        errors.len(),
        1,
        "relative path escaping spec_root should produce E025"
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn resolve_directory_to_index_spec() {
    let dir = setup_project(&[
        (
            "models/index.spec",
            r#"behavior model "M" { contract "model" }"#,
        ),
        (
            "main.spec",
            "use \"models\"\nbehavior caller \"C\" { invariants [model] }",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "directory import should resolve to index.spec: {:?}",
        diagnostics
    );
    // main.spec + models/index.spec = 2 files
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn bare_path_precedence_over_index() {
    let dir = setup_project(&[
        (
            "models.spec",
            r#"behavior direct_model "DM" { contract "direct" }"#,
        ),
        (
            "models/index.spec",
            r#"behavior index_model "IM" { contract "index" }"#,
        ),
        (
            "main.spec",
            "use \"models\"\nbehavior caller \"C\" { invariants [direct_model] }",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "models.spec should take precedence over models/index.spec: {:?}",
        diagnostics
    );
    // The import from main.spec resolves to models.spec, not models/index.spec.
    assert_eq!(
        import_target(
            &dir.iter().map(|(key, _)| *key).collect(),
            "main.spec",
            "models"
        ),
        Some("models.spec")
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "resolve extension import path"
)]
fn extension_import_emits_i004() {
    let dir = setup_project(&[(
        "main.spec",
        "use \"@specforge/software\"\nbehavior foo \"F\" { }",
    )]);

    let diagnostics = resolve(&dir, &[]);

    let infos: Vec<_> = diagnostics.iter().filter(|d| d.code == "I004").collect();
    assert_eq!(
        infos.len(),
        1,
        "uninstalled extension import should produce I004"
    );
    assert!(
        infos[0].message.contains("specforge") && infos[0].message.contains("software"),
        "I004 message should mention scope and name"
    );
    // No E025 should be emitted for extension imports
    assert!(
        diagnostics.iter().all(|d| d.code != "E025"),
        "extension import should not produce E025"
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "missing import file produces E025"
)]
fn missing_import_e025_with_suggestion() {
    let dir = setup_project(&[
        ("helpers.spec", r#"behavior helper "H" { contract "help" }"#),
        ("main.spec", "use \"helperz\"\nbehavior foo \"F\" { }"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .suggestion
            .as_ref()
            .is_some_and(|s| s.contains("helpers")),
        "E025 should suggest close match 'helpers': {:?}",
        errors[0].suggestion
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "missing import file produces E025"
)]
fn missing_import_e025_no_suggestion() {
    let dir = setup_project(&[
        ("types.spec", r#"behavior t "T" { contract "t" }"#),
        (
            "main.spec",
            "use \"zzzzz_completely_unrelated\"\nbehavior foo \"F\" { }",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(errors.len(), 1);
    // A distant path suggests no file, but states the rule (ADR 0055 D4).
    assert!(
        errors[0]
            .suggestion
            .as_deref()
            .is_some_and(|s| s.starts_with("a use path names a project source")),
        "distant path should state the rule: {:?}",
        errors[0].suggestion
    );
    assert!(
        matches!(
            errors[0].data.as_deref(),
            Some(specforge_common::DiagnosticData::UnresolvedImport {
                did_you_mean: None,
                ..
            })
        ),
        "{:?}",
        errors[0].data
    );
}

// --- pub use re-export scope tests ---

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "selective re-export of unknown entity produces W027"
)]
fn pub_use_unknown_entity_w027() {
    let dir = setup_project(&[
        ("foo.spec", r#"behavior Foo "F" { contract "foo" }"#),
        ("barrel.spec", "pub use { NonExistent } from \"./foo\"\n"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W027").collect();
    assert_eq!(
        warnings.len(),
        1,
        "should produce W027 for unknown selective re-export"
    );
    assert!(warnings[0].message.contains("NonExistent"));
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "relative import traversing above spec_root is rejected"
)]
fn relative_import_path_traversal_rejected() {
    // A spec file that tries to import ../../outside (above the spec root);
    // no such file is among the sources, whatever is on disk.
    let diagnostics = resolve(
        &[(
            "sub/main.spec",
            "use \"../../outside\"\nbehavior inner \"I\" { }",
        )],
        &[],
    );

    let e025: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert!(
        !e025.is_empty(),
        "import traversing above spec_root should produce E025"
    );
}

// === M3: W113 import cycle diagnostic carries suggestion ===

#[specforge_test(
    behavior = "detect_import_cycles",
    verify = "W113 carries actionable suggestion"
)]
fn w113_import_cycle_has_suggestion() {
    let dir = setup_project(&[
        ("a.spec", "use \"b\"\nbehavior alpha \"A\" { }"),
        ("b.spec", "use \"a\"\nbehavior beta \"B\" { }"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let w113s: Vec<_> = diagnostics.iter().filter(|d| d.code == "W113").collect();
    assert!(!w113s.is_empty(), "import cycle should produce W113");
    assert!(
        w113s[0].suggestion.is_some(),
        "W113 should carry an actionable suggestion, got None"
    );
    assert!(
        w113s[0].suggestion.as_ref().unwrap().contains("break"),
        "W113 suggestion should advise breaking the cycle, got: {:?}",
        w113s[0].suggestion
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import path may spell out the .spec extension"
)]
fn use_import_may_spell_out_the_spec_extension() {
    let dir = setup_project(&[
        ("tokens.spec", r#"behavior token "T" { contract "t" }"#),
        ("main.spec", "use \"tokens.spec\"\nbehavior foo \"F\" { }"),
        ("other.spec", "use \"tokens\"\nbehavior bar \"B\" { }"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        !diagnostics.iter().any(|d| d.code == "E025"),
        "{:?}",
        diagnostics
    );
}

/// W113 names each cycle from its smallest path, and the cycles come in
/// sorted order, whatever order the files were walked in (ADR 0004 D1-a).
#[specforge_test(
    invariant = "import_dag",
    verify = "a circular import produces W113 naming the cycle participants in a deterministic order"
)]
fn w113_names_each_cycle_the_same_way_on_every_run() {
    let dir = setup_project(&[
        ("c.spec", "use \"a\"\nbehavior gamma \"G\" { }"),
        ("a.spec", "use \"b\"\nbehavior alpha \"A\" { }"),
        ("b.spec", "use \"c\"\nbehavior beta \"B\" { }"),
        ("y.spec", "use \"x\"\nbehavior yankee \"Y\" { }"),
        ("x.spec", "use \"y\"\nbehavior xray \"X\" { }"),
    ]);

    for _ in 0..20 {
        let diagnostics = resolve(&dir, &[]);
        let messages: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "W113")
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "circular import detected: a.spec -> b.spec -> c.spec",
                "circular import detected: x.spec -> y.spec",
            ]
        );
    }
}

/// One import, resolved on its own (the LSP's go-to-definition on a `use`
/// path), the way the compile resolves it: bare, relative and
/// directory-index targets, relative to the spec root.
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a use path resolves to the project source it names"
)]
fn resolve_one_import_by_the_compile_cascade() {
    let sources = BTreeSet::from([
        "types.spec",
        "models/index.spec",
        "lib/shared/utils.spec",
        "sub/main.spec",
    ]);
    let resolve = |import: &str| import_target(&sources, "sub/main.spec", import);

    assert_eq!(resolve("types"), Some("types.spec"));
    assert_eq!(resolve("types.spec"), Some("types.spec"));
    assert_eq!(resolve("../types"), Some("types.spec"));
    assert_eq!(resolve("./main"), Some("sub/main.spec"));
    assert_eq!(resolve("models"), Some("models/index.spec"));
    assert_eq!(resolve("@shared/utils"), None, "an extension import");
    assert_eq!(resolve("@specforge/software"), None, "an extension");
    assert_eq!(resolve("missing"), None);
}

/// Whichever cascade step names it, a target outside the spec root does
/// not resolve: E025, as for a relative import.
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import reaching above spec_root by any cascade step produces E025"
)]
fn no_import_reaches_above_the_spec_root() {
    let sources = BTreeSet::from(["main.spec", "outside.spec"]);
    for import in ["../outside", "sub/../../outside", "/outside"] {
        assert_eq!(import_target(&sources, "main.spec", import), None);
        let diagnostics = resolve(
            &[
                (
                    "main.spec",
                    &format!("use \"{import}\"\nterm main \"Main\" {{\n}}\n"),
                ),
                ("outside.spec", "term o \"O\" {\n}\n"),
            ],
            &[],
        );
        assert!(
            diagnostics.iter().any(|d| d.code == "E025"),
            "{import}: {diagnostics:?}"
        );
    }
}

// --- W027 pins: the re-export obligations, observed without the file scopes ---

/// The W027 messages the project of `files` reports.
fn w027_messages(files: &[(&str, &str)]) -> Vec<String> {
    resolve(files, &[])
        .iter()
        .filter(|d| d.code == "W027")
        .map(|d| d.message.clone())
        .collect()
}

const USER_AND_PROFILE: &str = "behavior User \"U\" { contract \"user\" }\nbehavior UserProfile \"UP\" { contract \"profile\" }";

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "pub use re-exports all entities from target"
)]
fn pub_use_reexports_all_entities_w027() {
    // top names UserProfile through barrel's `pub use "./user"`.
    let w027 = w027_messages(&[
        ("user.spec", USER_AND_PROFILE),
        ("barrel.spec", "pub use \"./user\"\n"),
        ("top.spec", "pub use { UserProfile } from \"./barrel\"\n"),
    ]);
    assert!(w027.is_empty(), "{w027:?}");
}

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "pub use selective re-exports only named entities"
)]
fn pub_use_selective_reexport_w027() {
    let w027 = w027_messages(&[
        ("user.spec", USER_AND_PROFILE),
        ("barrel.spec", "pub use { User } from \"./user\"\n"),
        (
            "top.spec",
            "pub use { User, UserProfile } from \"./barrel\"\n",
        ),
    ]);
    assert_eq!(
        w027,
        ["selective re-export 'UserProfile' not found in target 'barrel.spec'"]
    );
}

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "pub use chains resolve transitively"
)]
fn pub_use_transitive_chain_w027() {
    let w027 = w027_messages(&[
        (
            "deep.spec",
            "behavior DeepEntity \"D\" { contract \"deep\" }",
        ),
        ("mid.spec", "pub use \"./deep\"\n"),
        ("top.spec", "pub use \"./mid\"\n"),
        ("leaf.spec", "pub use { DeepEntity } from \"./top\"\n"),
    ]);
    assert!(w027.is_empty(), "{w027:?}");
}

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "regular use does not re-export"
)]
fn regular_use_does_not_reexport_w027() {
    let w027 = w027_messages(&[
        ("user.spec", "behavior User \"U\" { contract \"user\" }"),
        (
            "consumer.spec",
            "use \"./user\"\nbehavior Consumer \"C\" { invariants [User] }",
        ),
        (
            "leaf.spec",
            "pub use { User, Consumer } from \"./consumer\"\n",
        ),
    ]);
    assert_eq!(w027.len(), 1, "{w027:?}");
    assert!(w027[0].contains("'User'"), "{w027:?}");
}

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "barrel index with pub use re-exports from sub-files"
)]
fn barrel_index_with_pub_use_w027() {
    let w027 = w027_messages(&[
        (
            "models/user.spec",
            "behavior User \"U\" { contract \"user\" }",
        ),
        (
            "models/order.spec",
            "behavior Order \"O\" { contract \"order\" }",
        ),
        (
            "models/index.spec",
            "pub use \"./user\"\npub use \"./order\"\n",
        ),
        ("top.spec", "pub use { User, Order } from \"./models\"\n"),
    ]);
    assert!(w027.is_empty(), "{w027:?}");
}

#[specforge_test(
    behavior = "resolve_reexports",
    verify = "pub use through cycle participant uses only declared set"
)]
fn pub_use_through_cycle_no_transitive_w027() {
    // a and b form a cycle; c pub-uses a; d names what c exports.
    let w027 = w027_messages(&[
        (
            "a.spec",
            "use \"./b\"\npub use \"./b\"\nbehavior Alpha \"A\" { }",
        ),
        ("b.spec", "use \"./a\"\nbehavior Beta \"B\" { }"),
        ("c.spec", "pub use \"./a\"\nbehavior Gamma \"G\" { }"),
        ("d.spec", "pub use { Alpha, Gamma, Beta } from \"./c\"\n"),
    ]);
    assert_eq!(w027.len(), 1, "{w027:?}");
    assert!(w027[0].contains("'Beta'"), "{w027:?}");
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a dotted import path names the file with .spec appended"
)]
fn a_dotted_import_path_names_the_file_with_spec_appended() {
    let dir = setup_project(&[
        ("api.v2.spec", "term v \"V\" {\n}\n"),
        ("api.spec", "term w \"W\" {\n}\n"),
        (
            "main.spec",
            "pub use { v } from \"api.v2\"\nuse \"api.v2\"\n",
        ),
    ]);

    let diagnostics = resolve(&dir, &[]);

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == "E025" || d.code == "W027"),
        "{diagnostics:?}"
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "import paths match source paths exactly, whatever the file system's case rules"
)]
fn import_paths_match_source_paths_exactly() {
    let dir = setup_project(&[
        ("types.spec", "term t \"T\" {\n}\n"),
        ("main.spec", "use \"Types\"\n"),
    ]);

    let diagnostics = resolve(&dir, &[]);

    let e025: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(e025.len(), 1, "{diagnostics:?}");
    assert!(
        matches!(
            e025[0].data.as_deref(),
            Some(specforge_common::DiagnosticData::UnresolvedImport { path, did_you_mean })
                if path == "Types" && did_you_mean.as_deref() == Some("types")
        ),
        "{:?}",
        e025[0].data
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import of a file that is no project source produces E025"
)]
fn an_import_of_a_key_no_source_has_is_e025() {
    let diagnostics = resolve(&[("main.spec", "use \"drafts/x\"\n")], &[]);

    let e025: Vec<_> = diagnostics.iter().filter(|d| d.code == "E025").collect();
    assert_eq!(e025.len(), 1, "{diagnostics:?}");
    assert_eq!(
        e025[0].message,
        "import target not found among the project's sources: drafts/x"
    );
    assert_eq!(
        e025[0].suggestion.as_deref(),
        Some(
            "a use path names a project source: a .spec file under the spec root, not excluded and not under a skipped directory"
        )
    );
}

#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "an import of an unreadable source reports only that source's E025"
)]
fn an_unreadable_key_is_a_source_with_no_exports() {
    let diagnostics = resolve(
        &[(
            "c.spec",
            "use \"bad\"\npub use { q } from \"bad\"\nterm c \"C\" {\n}\n",
        )],
        &["bad.spec"],
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
