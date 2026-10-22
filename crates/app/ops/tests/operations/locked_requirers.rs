//! A locked extension that requires a peer is a member of the project's peer set only as its module
//! declares it (ADR 0217): the refusals `add` and `remove` give for a locked requirer, with a module
//! that declares what its lock entry records. (A binary cannot be handed a requirer a runtime does
//! not serve, so these run in process.)

use std::path::Path;
use std::sync::Arc;

use specforge_ops::extension::{self, AddRequest, RemoveRequest};
use specforge_ops::registry::testing::{MemoryRegistry, Published, declaration};
use specforge_ops::testing::{GREET, candidates, declaring, probes};
use specforge_ops::view::ProjectView;
use specforge_project::testing::compile_at;
use specforge_test_macros::test as specforge_test;
use specforge_wasm::testing::InProcessRuntime;

const USES_GREET: &[u8] = b"\0asm in-process @acme/uses-greet 1.0.0";
const BASE: &[u8] = b"\0asm in-process @acme/base 1.0.0";
const OTHER: &[u8] = b"\0asm in-process @acme/other 1.0.0";

/// A project enabling `extensions`.
fn project(extensions: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config = serde_json::json!({"name": "p", "version": "0.1.0", "extensions": extensions});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
}

/// Every file under `root` with its bytes.
fn snapshot(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

fn enabled(root: &Path) -> Vec<String> {
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("specforge.json")).unwrap())
            .unwrap();
    serde_json::from_value(config["extensions"].clone()).unwrap()
}

/// `@acme/uses-greet`, requiring `@sdk/greet` ^0.1.0, beside the greet binary.
fn uses_greet() -> InProcessRuntime {
    let peers = &[("@sdk/greet", "^0.1.0", false)];
    candidates().binary(USES_GREET, declaring("@acme/uses-greet", "1.0.0", peers))
}

fn add(
    root: &Path,
    specifier: &str,
    registry: &MemoryRegistry,
    runtime: &InProcessRuntime,
) -> Result<extension::Added, specforge_ops::Refusal<extension::AddField>> {
    extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            extensions: vec![specifier.to_string()],
            base: None,
            allow_unsigned: true,
            dry_run: false,
            caller: specforge_ops::testing::caller(),
        },
        registry,
        runtime,
    )
}

fn remove(
    root: &Path,
    name: &str,
    force: bool,
    runtime: InProcessRuntime,
) -> Result<extension::RemoveOutcome, specforge_ops::Refusal<extension::RemoveField>> {
    let compiled = compile_at(root, Some(Arc::new(runtime)));
    extension::remove(
        &ProjectView::of(&compiled),
        &RemoveRequest {
            name,
            force,
            dry_run: false,
            caller: specforge_ops::testing::caller(),
        },
    )
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "removing an extension another installed extension requires fails with E027 unless --force"
)]
fn removing_an_extension_another_installed_extension_requires_is_e027_unless_forced() {
    let dir = project(&["@probe/base", "@probe/dep"]);
    for (name, bytes) in [
        ("@probe/base", specforge_ops::testing::BASE_1),
        ("@probe/dep", specforge_ops::testing::DEP_1),
    ] {
        specforge_installed::testing::install_module(dir.path(), &probes(), name, bytes);
    }
    let before = snapshot(dir.path());

    let error = remove(dir.path(), "@probe/base", false, probes()).unwrap_err();

    assert_eq!(error.error.code, "E027", "{error:?}");
    assert!(error.error.message.contains("@probe/dep"), "{error:?}");
    assert_eq!(snapshot(dir.path()), before, "nothing changed");

    remove(dir.path(), "@probe/base", true, probes()).unwrap();
    assert_eq!(enabled(dir.path()), ["@probe/dep"]);
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "removing a .wasm file entry another extension requires fails with E027 unless --force"
)]
fn removing_a_wasm_file_entry_another_extension_requires_is_e027_unless_forced() {
    let dir = project(&["greet.wasm", "@acme/uses-greet"]);
    std::fs::write(dir.path().join("greet.wasm"), GREET).unwrap();
    specforge_installed::testing::install_module(
        dir.path(),
        &uses_greet(),
        "@acme/uses-greet",
        USES_GREET,
    );
    let before = snapshot(dir.path());

    let error = remove(dir.path(), "greet.wasm", false, uses_greet()).unwrap_err();

    assert_eq!(error.error.code, "E027", "{error:?}");
    assert!(
        error.error.message.contains("@acme/uses-greet"),
        "{error:?}"
    );
    assert_eq!(snapshot(dir.path()), before, "nothing changed");

    remove(dir.path(), "@sdk/greet", true, uses_greet()).unwrap();
    assert_eq!(enabled(dir.path()), ["@acme/uses-greet"]);
    assert!(dir.path().join("greet.wasm").is_file());
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an install that leaves a locked extension's peer unsatisfied is refused before anything is written, local or from a registry"
)]
fn an_install_a_locked_extension_does_not_accept_is_refused_local_or_from_a_registry() {
    let dir = project(&["@acme/app"]);
    let runtime = || {
        candidates().binary(
            USES_GREET,
            declaring("@acme/app", "1.0.0", &[("@sdk/greet", "^2.0", false)]),
        )
    };
    specforge_installed::testing::install_module(dir.path(), &runtime(), "@acme/app", USES_GREET);
    let local = dir.path().join("greet.wasm");
    std::fs::write(&local, GREET).unwrap();
    let before = snapshot(dir.path());

    let error = add(
        dir.path(),
        &local.display().to_string(),
        &MemoryRegistry::new(),
        &runtime(),
    )
    .unwrap_err();
    assert_eq!(error.error.code, "E027", "{error:?}");
    assert!(
        error
            .error
            .message
            .contains("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
        "{error:?}"
    );
    assert_eq!(snapshot(dir.path()), before, "nothing is written");

    // `@acme/app` wants `@sdk/greet ^2.0`, which the registry never published.
    let registry = MemoryRegistry::new().serving(Published::new(
        GREET.to_vec(),
        declaration("@sdk/greet", "0.1.0", &[]),
    ));
    let error = add(dir.path(), "@sdk/greet@0.1.0", &registry, &runtime()).unwrap_err();
    assert_eq!(error.error.code, "R-RES-005", "{error:?}");
    let message = &error.error.message;
    assert!(
        message.contains("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
        "{message}"
    );
    assert_eq!(
        message.matches("@acme/app wants @sdk/greet ^2.0").count(),
        1,
        "{message}"
    );
    assert_eq!(snapshot(dir.path()), before, "nothing is installed");
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "a version diamond with a locked peer is refused with R-RES-006, as specforge add refuses it"
)]
fn a_version_diamond_with_a_locked_peer_is_refused_with_r_res_006() {
    let dir = project(&["@acme/base", "@acme/other"]);
    // `@acme/base` is locked at 1.0.0 for `@acme/other`, which takes any 1+.
    let runtime = || {
        InProcessRuntime::new()
            .binary(BASE, declaring("@acme/base", "1.0.0", &[]))
            .binary(
                OTHER,
                declaring("@acme/other", "1.0.0", &[("@acme/base", ">=1.0.0", false)]),
            )
    };
    for (name, bytes) in [("@acme/base", BASE), ("@acme/other", OTHER)] {
        specforge_installed::testing::install_module(dir.path(), &runtime(), name, bytes);
    }
    let registry = MemoryRegistry::new()
        .serving(Published::new(
            b"base".to_vec(),
            declaration("@acme/base", "1.0.0", &[]),
        ))
        .serving(Published::new(
            b"base".to_vec(),
            declaration("@acme/base", "2.0.0", &[]),
        ))
        .serving(Published::new(
            b"app".to_vec(),
            declaration("@acme/app", "1.0.0", &[("@acme/base", "^2.0")]),
        ));
    let before = snapshot(dir.path());

    let error = add(dir.path(), "@acme/app@1.0.0", &registry, &runtime()).unwrap_err();

    assert_eq!(error.error.code, "R-RES-006", "{error:?}");
    assert!(
        error.error.message.contains("@acme/base 2.0.0"),
        "{error:?}"
    );
    assert_eq!(snapshot(dir.path()), before, "nothing is installed");
}
