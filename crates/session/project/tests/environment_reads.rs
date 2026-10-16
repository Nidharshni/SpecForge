//! What a session's environment inputs are: the reads its environment load made
//! (behavior `classify_project_changes`, ADR 0114). The pins here are ignored
//! until the ticket that fixes them.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_common::ProjectRoot;
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
#[ignore = "flipped by T7"]
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
#[ignore = "flipped by T7"]
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
