use specforge_common::{Diagnostic, DiagnosticData, Severity};
use specforge_parser::{SpecFile, parse};
use specforge_resolver::resolve_imports;
use specforge_test_macros::test as specforge_test;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// The import diagnostics of the project under `root`, read and parsed as a
/// compile reads it.
fn resolve_dir(root: &Path) -> Vec<Diagnostic> {
    let parsed: Vec<(String, SpecFile)> = specforge_common::discover_spec_files(root, &[])
        .into_iter()
        .map(|p| {
            let key = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            let spec = parse(&fs::read_to_string(&p).unwrap(), &key);
            (key, spec)
        })
        .collect();
    let files: Vec<(&str, &SpecFile)> = parsed.iter().map(|(k, f)| (k.as_str(), f)).collect();
    resolve_imports(root, &files, &|p: &Path| p.is_file())
}

fn setup_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    dir
}

// B:resolve_use_imports — verify contract "requires/ensures consistency for use import resolution"
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "Resolve Use Imports: use import resolution holds — registries_populated_fired, sources_held, imports_resolved, missing_files_diagnosed, dependency_graph_built"
)]
fn resolve_use_imports_contract() {
    // Requires sources_held: the project lives on disk. main.spec
    // imports two files, one nested, and one that does not exist.
    let dir = setup_project(&[
        ("types.spec", r#"behavior alpha "A" { contract "first" }"#),
        ("models/user.spec", r#"behavior gamma "G" { contract "g" }"#),
        (
            "main.spec",
            "use \"types\"\nuse \"models/user\"\nuse \"missing\"\nbehavior beta \"B\" { invariants [alpha] }",
        ),
    ]);

    let diagnostics = resolve_dir(dir.path());

    // imports_resolved + dependency_graph_built: `use "types"` and
    // `use "models/user"` resolve (no E025 for them), and the files they
    // name are the targets of the import graph: a file importing main.spec
    // back closes the cycle main.spec -> models/user.spec.
    assert!(
        !diagnostics.iter().any(
            |d| d.code == "E025" && (d.message.contains("types") || d.message.contains("user"))
        ),
        "{diagnostics:?}"
    );
    fs::write(
        dir.path().join("models/user.spec"),
        "use \"main\"\nbehavior gamma \"G\" { contract \"g\" }",
    )
    .unwrap();
    let cyclic = resolve_dir(dir.path());
    let w113: Vec<&str> = cyclic
        .iter()
        .filter(|d| d.code == "W113")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        w113,
        ["circular import detected: main.spec -> models/user.spec"]
    );

    // missing_files_diagnosed: the one missing import, and nothing else.
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, "E025");
    assert!(
        errors[0].message.contains("missing"),
        "{}",
        errors[0].message
    );
    let span = errors[0].span.as_ref().expect("E025 points at the use");
    assert_eq!(span.file.as_str(), "main.spec");
    assert_eq!(span.start_line, 3);
    // The path it names is data too, so no consumer parses the message.
    assert!(
        matches!(
            errors[0].data.as_deref(),
            Some(DiagnosticData::UnresolvedImport { path, .. }) if path == "missing"
        ),
        "{:?}",
        errors[0].data
    );
}

// B:detect_import_cycles — verify contract "requires/ensures consistency for import cycle detection"
#[specforge_test(
    behavior = "detect_import_cycles",
    verify = "Detect Import Cycles: import cycle detection holds — import_graph_available, cycles_detected, cycle_diagnostic_emitted, non_cyclic_unaffected"
)]
fn detect_import_cycles_contract() {
    // Requires: project with circular imports
    // Ensures: W113 cycle diagnostic produced as warning (not error)
    let dir = setup_project(&[
        ("a.spec", "use \"b\"\nbehavior alpha \"A\" { }"),
        ("b.spec", "use \"a\"\nbehavior beta \"B\" { }"),
    ]);

    let diagnostics = resolve_dir(dir.path());

    let cycle_warnings: Vec<_> = diagnostics.iter().filter(|d| d.code == "W113").collect();
    assert!(
        !cycle_warnings.is_empty(),
        "circular imports must produce W113"
    );
    assert!(
        cycle_warnings
            .iter()
            .all(|d| d.severity == Severity::Warning),
        "import cycles must be warnings, not errors"
    );
}
