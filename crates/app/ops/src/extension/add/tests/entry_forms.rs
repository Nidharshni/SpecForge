use super::*;
use crate::config::testing::files_under;

/// A project enabling `entries`, with the greet component at `a.wasm`.
fn project_enabling(entries: &[&str]) -> tempfile::TempDir {
    let dir = project_with(&serde_json::json!({ "name": "p", "extensions": entries }).to_string());
    std::fs::write(dir.path().join("a.wasm"), GREET).unwrap();
    dir
}

/// `add <wasm>` into `dir`.
fn add_file(dir: &Path, wasm: &Path, dry_run: bool) -> Result<Added, Refusal<AddField>> {
    let root = specforge_common::ProjectRoot::of(dir).unwrap();
    let mut request = local_request(&root, wasm);
    request.dry_run = dry_run;
    add(&request, &crate::registry::Unconfigured("add"), &runtime())
}

/// What an add over a `.wasm` file entry that loads `@sdk/greet` answers (ADR 0160).
fn assert_loaded_from_the_file(error: &Refusal<AddField>) {
    assert_eq!(error.error.code, "extension_conflict", "{error:?}");
    assert_eq!(error.error.kind, OpErrorKind::Conflict);
    assert_eq!(
        error.error.message,
        "'@sdk/greet' is loaded from the .wasm file entry './a.wasm' in specforge.json"
    );
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("remove that entry first: specforge remove ./a.wasm")
    );
}

/// Adding greet over `entries` is refused, dry run or not, and writes nothing.
fn assert_refused_over(entries: &[&str]) {
    let dir = project_enabling(entries);
    let (_files, wasm) = greet_file();
    let before = files_under(dir.path());
    for dry_run in [true, false] {
        assert_loaded_from_the_file(&add_file(dir.path(), &wasm, dry_run).unwrap_err());
        assert_eq!(files_under(dir.path()), before);
    }
}

#[test]
fn an_add_of_what_a_name_file_entry_loads_is_refused() {
    assert_refused_over(&["@sdk/greet=./a.wasm"]);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an add of an extension a .wasm file entry loads is refused, naming the entry, and writes nothing"
)]
fn an_add_of_what_a_bare_file_entry_loads_is_refused() {
    assert_refused_over(&["./a.wasm"]);
}

#[test]
fn an_add_that_would_shadow_a_loading_file_entry_is_refused() {
    assert_refused_over(&["@sdk/greet", "./a.wasm"]);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a .wasm file entry never makes an install already present"
)]
fn a_file_entry_never_makes_an_install_present() {
    let dir = project_enabling(&[]);
    let (_files, wasm) = greet_file();
    add_file(dir.path(), &wasm, false).unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","extensions":["@sdk/greet=./a.wasm"]}"#,
    )
    .unwrap();
    let before = files_under(dir.path());

    assert_loaded_from_the_file(&add_file(dir.path(), &wasm, false).unwrap_err());
    assert_eq!(files_under(dir.path()), before);
}

/// Installing greet over `entry` writes the module and the lock, appends no entry, and a second
/// add finds it present.
fn assert_named_entry_is_enough(entry: &str) {
    let dir = project_enabling(&[entry]);
    let (_files, wasm) = greet_file();

    let first = add_file(dir.path(), &wasm, false).unwrap();
    assert!(
        matches!(first.added[0].outcome, AddOutcome::Installed { .. }),
        "{:?}",
        first.added[0].outcome
    );
    assert_eq!(first.writes.len(), 2, "the module and specforge.lock");
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("specforge.json")).unwrap()).unwrap();
    assert_eq!(config["extensions"], serde_json::json!([entry]));

    let second = add_file(dir.path(), &wasm, false).unwrap();
    let AddOutcome::AlreadyPresent { version, .. } = &second.added[0].outcome else {
        panic!("{:?}", second.added[0].outcome);
    };
    assert_eq!(version, "0.1.0");
    assert!(second.writes.is_empty());
}

#[test]
fn a_legacy_versioned_entry_enables_and_makes_present_the_install_it_names() {
    assert_named_entry_is_enough("@sdk/greet@0.1.0");
}

#[test]
fn a_named_entry_not_yet_installed_is_installed_without_a_second_entry() {
    assert_named_entry_is_enough("@sdk/greet");
}
