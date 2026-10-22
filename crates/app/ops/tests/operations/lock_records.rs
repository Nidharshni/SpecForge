//! A locked extension is judged by what its module declares, and its lock entry must record that
//! (ADR 0217): the lock plan and `check` agree, and an add rewrites an entry that does not.

use std::path::Path;
use std::sync::Arc;

use specforge_ops::extension::{self, AddOutcome, AddRequest, Added};
use specforge_ops::testing::{BASE_2, DEP_1, probes};
use specforge_project::testing::compile_at;
use specforge_test_macros::test as specforge_test;

fn project(extensions: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let config = serde_json::json!({"name": "p", "version": "0.1.0", "extensions": extensions});
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    dir
}

/// Rewrite the lock entry of `name` to record `version` and `peers`.
fn lock_record(root: &Path, name: &str, version: &str, peers: &[(&str, &str)]) {
    let path = root.join("specforge.lock");
    let mut lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for entry in lock["entries"].as_array_mut().unwrap() {
        if entry["name"] == name {
            entry["version"] = version.into();
            entry["peer_dependencies"] = peers
                .iter()
                .map(|(peer, range)| {
                    serde_json::json!({"name": peer, "version": range, "optional": false})
                })
                .collect();
        }
    }
    std::fs::write(&path, lock.to_string()).unwrap();
}

fn add(root: &Path, source: &str) -> Result<Added, specforge_ops::Refusal<extension::AddField>> {
    extension::add(
        &AddRequest {
            root: &specforge_common::ProjectRoot::of(root).unwrap(),
            extensions: vec![source.to_string()],
            base: None,
            allow_unsigned: false,
            dry_run: false,
            caller: specforge_ops::testing::caller(),
        },
        &specforge_ops::registry::Unconfigured("test"),
        &probes(),
    )
}

/// The codes of the compiled project's diagnostics, with the messages of the E070 among them.
fn compiled(root: &Path) -> (Vec<String>, Vec<String>) {
    let compiled = compile_at(root, Some(Arc::new(probes())));
    let all = compiled.diagnostics();
    let codes = all.iter().map(|d| d.code.clone()).collect();
    let e070 = all
        .iter()
        .filter(|d| d.code == "E070")
        .map(|d| d.message.clone())
        .collect();
    (codes, e070)
}

#[specforge_test(
    invariant = "peer_dependency_satisfaction",
    verify = "the lock plan judges a locked extension by what its module declares, and one whose lock entry records another version or other peers is no member, as the load refuses it"
)]
#[ignore = "flipped by T5"]
fn the_lock_plan_judges_a_locked_module_by_what_it_declares() {
    // Forward: the entry records a peer the module never declared.
    let dir = project(&["@probe/base"]);
    specforge_installed::testing::install_module(dir.path(), &probes(), "@probe/base", BASE_2);
    lock_record(
        dir.path(),
        "@probe/base",
        "2.0.0",
        &[("@specforge/software", "^9.0")],
    );
    add(dir.path(), "@specforge/software")
        .unwrap_or_else(|error| panic!("forward: the copy steers the gate: {error:?}"));
    let (codes, e070) = compiled(dir.path());
    assert!(
        !codes.iter().any(|c| c == "E027" || c == "E073"),
        "{codes:?}"
    );
    assert!(e070.iter().any(|m| m.contains("@probe/base")), "{e070:?}");

    // Reverse: the entry records no peer the module declares.
    let dir = project(&["@probe/dep"]);
    specforge_installed::testing::install_module(dir.path(), &probes(), "@probe/dep", DEP_1);
    lock_record(dir.path(), "@probe/dep", "1.0.0", &[]);
    let base = tempfile::tempdir().unwrap();
    std::fs::write(base.path().join("base.wasm"), BASE_2).unwrap();
    add(
        dir.path(),
        &base.path().join("base.wasm").display().to_string(),
    )
    .unwrap_or_else(|error| panic!("reverse: the add passes: {error:?}"));
    let (codes, e070) = compiled(dir.path());
    assert!(!codes.iter().any(|c| c == "E027"), "{codes:?}");
    assert!(e070.iter().any(|m| m.contains("@probe/dep")), "{e070:?}");

    // Control: the entry records what the module declares, so the gate refuses the base.
    let dir = project(&["@probe/dep"]);
    specforge_installed::testing::install_module(dir.path(), &probes(), "@probe/dep", DEP_1);
    lock_record(
        dir.path(),
        "@probe/dep",
        "1.0.0",
        &[("@probe/base", "^1.0")],
    );
    let error = add(
        dir.path(),
        &base.path().join("base.wasm").display().to_string(),
    )
    .unwrap_err();
    assert_eq!(error.error.code, "E027", "{error:?}");
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an add of an installed extension whose lock entry records other than its module declares rewrites the entry from the module"
)]
#[ignore = "flipped by T6"]
fn an_add_rewrites_a_lock_entry_its_module_does_not_declare() {
    let dir = project(&[]);
    let file = dir.path().join("base.wasm");
    std::fs::write(&file, BASE_2).unwrap();
    let source = file.display().to_string();
    add(dir.path(), &source).unwrap();
    lock_record(
        dir.path(),
        "@probe/base",
        "2.0.0",
        &[("@specforge/software", "^9.0")],
    );

    let again = add(dir.path(), &source).unwrap();

    assert!(
        matches!(again.added[0].outcome, AddOutcome::Installed { .. }),
        "{:?}",
        again.added[0].outcome
    );
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("specforge.lock")).unwrap())
            .unwrap();
    assert!(
        lock["entries"][0].get("peer_dependencies").is_none(),
        "{lock}"
    );
    let (codes, _) = compiled(dir.path());
    assert!(
        !codes.iter().any(|c| c == "E027" || c == "E070"),
        "{codes:?}"
    );
}
