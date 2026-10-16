//! What a session's environment inputs are: the reads its environment load made
//! (behavior `classify_project_changes`, ADR 0114). The pins here are ignored
//! until the ticket that fixes them.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use specforge_common::ProjectRoot;
use specforge_project::testing::TestProject;
use specforge_project::{ProjectSession, SharedRuntime, WatchRoot, Watched};
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

/// A session whose extension load runs (every module is read; each fails as
/// no component, which is fine here).
fn loading(root: &Path) -> ProjectSession {
    ProjectSession::open_with_runtime(
        &ProjectRoot::of(root).unwrap(),
        Some(Arc::new(InProcessRuntime::new()) as SharedRuntime),
    )
}

/// `t/proj` (the root) and `t/outside`, both canonical.
struct Layout {
    _t: TempDir,
    root: PathBuf,
    outside: PathBuf,
}

fn layout() -> Layout {
    let t = TempDir::new().unwrap();
    let base = fs::canonicalize(t.path()).unwrap();
    let root = base.join("proj");
    let outside = base.join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("a.spec"), "").unwrap();
    Layout {
        _t: t,
        root,
        outside,
    }
}

fn config_with_module(l: &Layout) {
    fs::write(
        l.root.join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@acme/x=tools/ext.wasm"]}"#,
    )
    .unwrap();
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an environment input that is a symlink is followed to its target"
)]
fn a_symlinked_modules_target_is_watched() {
    let l = layout();
    config_with_module(&l);
    fs::write(l.outside.join("ext.wasm"), b"first").unwrap();
    fs::create_dir_all(l.root.join("tools")).unwrap();
    std::os::unix::fs::symlink(l.outside.join("ext.wasm"), l.root.join("tools/ext.wasm")).unwrap();
    let session = loading(&l.root);

    assert!(
        session.inputs().watch_roots().contains(&WatchRoot {
            dir: l.outside.clone(),
            recursive: true
        }),
        "{:?}",
        session.inputs().watch_roots()
    );
    fs::write(l.outside.join("ext.wasm"), b"second, longer").unwrap();
    let changes = session.changes([l.outside.join("ext.wasm").as_path()]);
    assert!(!changes.environment.is_empty(), "{changes:?}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an environment input that is a symlink is followed to its target"
)]
fn a_symlinked_config_and_locks_targets_are_watched() {
    let l = layout();
    let base = l.root.parent().unwrap();
    let shared_config = base.join("shared_config");
    let shared_lock = base.join("shared_lock");
    fs::create_dir_all(&shared_config).unwrap();
    fs::create_dir_all(&shared_lock).unwrap();
    fs::write(
        shared_config.join("specforge.json"),
        r#"{"name":"p","version":"0.1.0"}"#,
    )
    .unwrap();
    fs::write(
        shared_lock.join("specforge.lock"),
        r#"{"lockfile_version":1,"entries":[]}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(
        shared_config.join("specforge.json"),
        l.root.join("specforge.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        shared_lock.join("specforge.lock"),
        l.root.join("specforge.lock"),
    )
    .unwrap();
    let session = loading(&l.root);

    let roots = session.inputs().watch_roots();
    for dir in [&shared_config, &shared_lock] {
        assert!(roots.iter().any(|w| &w.dir == dir), "{dir:?} in {roots:?}");
    }
}

#[cfg(unix)]
#[ignore = "flipped by T8"]
#[specforge_test(
    behavior = "classify_project_changes",
    verify = "an input that is a symlink pointed elsewhere is its change"
)]
fn a_module_link_pointed_elsewhere_reloads_the_environment() {
    let l = layout();
    config_with_module(&l);
    fs::write(l.outside.join("ext.wasm"), b"first").unwrap();
    fs::write(l.outside.join("other.wasm"), b"another module").unwrap();
    fs::create_dir_all(l.root.join("tools")).unwrap();
    let link = l.root.join("tools/ext.wasm");
    std::os::unix::fs::symlink(l.outside.join("ext.wasm"), &link).unwrap();
    let session = loading(&l.root);

    let watched = session.inputs().watched();
    assert!(
        watched.contains(&Watched::File(link.clone())),
        "{watched:?}"
    );
    assert!(
        watched.contains(&Watched::File(l.outside.join("ext.wasm"))),
        "{watched:?}"
    );

    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(l.outside.join("other.wasm"), &link).unwrap();
    let changes = session.changes([link.as_path()]);
    assert_eq!(changes.environment, vec![link], "{changes:?}");
}

fn project(config: serde_json::Value, files: &[(&str, &str)]) -> TestProject {
    TestProject::new()
        .raw_config(config.to_string())
        .files(files)
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a directory moved above an environment or check input changes that input"
)]
fn a_moved_directory_holding_a_module_reloads_the_environment() {
    let dir = project(
        json!({"name": "c", "version": "0.1.0", "extensions": ["@acme/local=ext/local.wasm"]}),
        &[("ext/local.wasm", "not wasm")],
    );
    let session = loading(dir.root());
    let root = fs::canonicalize(dir.root()).unwrap();
    fs::rename(root.join("ext"), root.join("ext2")).unwrap();

    let changes = session.changes([root.join("ext").as_path(), root.join("ext2").as_path()]);
    assert_eq!(changes.environment, vec![root.join("ext")]);
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "specforge.json, specforge.lock and a loaded extension module are environment changes"
)]
fn config_lock_and_loaded_modules_are_environment_inputs() {
    let dir = project(
        json!({
            "name": "c",
            "version": "0.1.0",
            "extensions": ["@specforge/software", "@acme/local=ext/local.wasm", "@acme/installed"]
        }),
        &[("main.spec", "")],
    );
    let session = loading(dir.root());
    let root = dir.root();

    // A builtin has no module on disk; a local entry does. An installed entry
    // with no lock entry is not loaded, so its module is read by no one.
    assert_eq!(
        session.inputs().watched(),
        vec![
            Watched::Sources(root.to_path_buf()),
            Watched::Directories(root.to_path_buf()),
            Watched::File(root.join("specforge.json")),
            Watched::File(root.join("specforge.lock")),
            Watched::File(root.join("ext/local.wasm")),
        ]
    );
    for path in ["specforge.json", "specforge.lock", "ext/local.wasm"] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "new content").unwrap();
        let changes = session.changes([path.as_path()]);
        assert_eq!(
            changes.environment,
            vec![path.clone()],
            "{}",
            path.display()
        );
        assert!(changes.sources.is_empty());
    }
    let unlocked = root.join(".specforge/extensions/@acme/installed/extension.wasm");
    fs::create_dir_all(unlocked.parent().unwrap()).unwrap();
    fs::write(&unlocked, "new content").unwrap();
    assert!(
        session.changes([unlocked.as_path()]).environment.is_empty(),
        "the load did not read it"
    );
}

/// The directories a watcher must watch: the root, a spec root outside
/// it, and the directory of a module outside both.
#[test]
fn watch_roots_cover_every_input() {
    let outside = TempDir::new().unwrap();
    let module_dir = fs::canonicalize(outside.path()).unwrap().join("mods");
    fs::create_dir_all(&module_dir).unwrap();
    let module = module_dir.join("ext.wasm");
    let dir = project(
        json!({
            "name": "c",
            "version": "0.1.0",
            "spec_root": "spec",
            "extensions": [format!("@acme/far={}", module.display())]
        }),
        &[("spec/a.spec", "")],
    );
    let session = loading(dir.root());
    let root = fs::canonicalize(dir.root()).unwrap();
    assert_eq!(
        session.inputs().watch_roots(),
        vec![
            WatchRoot {
                dir: root,
                recursive: true
            },
            WatchRoot {
                dir: module_dir,
                recursive: true
            },
        ]
    );
}

#[test]
fn an_entry_the_load_does_not_read_is_no_input() {
    let dir = project(
        json!({"name": "c", "version": "0.1.0", "extensions": ["@acme/unlocked"]}),
        &[("a.spec", "")],
    );
    let session = loading(dir.root());
    let root = dir.root();
    let unlocked = root.join(".specforge/extensions/@acme/unlocked/extension.wasm");
    assert!(
        !session
            .inputs()
            .watched()
            .contains(&Watched::File(unlocked)),
        "{:?}",
        session.inputs().watched()
    );
    // The lock is the input that changes it.
    fs::write(
        root.join("specforge.lock"),
        r#"{"lockfile_version":1,"entries":[]}"#,
    )
    .unwrap();
    let changes = session.changes([root.join("specforge.lock").as_path()]);
    assert_eq!(changes.environment, vec![root.join("specforge.lock")]);
}

#[test]
fn a_session_without_a_runtime_records_the_config_and_the_lock_only() {
    let dir = project(
        json!({"name": "c", "version": "0.1.0", "extensions": ["@acme/x=tools/ext.wasm"]}),
        &[("a.spec", ""), ("tools/ext.wasm", "module")],
    );
    let session = dir.session_bare();
    let root = dir.root();
    let files: Vec<_> = session
        .inputs()
        .watched()
        .into_iter()
        .filter_map(|w| match w {
            Watched::File(path) => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(
        files,
        vec![root.join("specforge.json"), root.join("specforge.lock")]
    );
}
