use specforge_test_macros::test as spec;
use specforge_watch::SpecWatcher;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// The paths a watcher reports: waits up to `timeout` for the first, then
/// takes what follows until the watcher is quiet for a moment.
fn wait_for_event(rx: &mpsc::Receiver<PathBuf>, timeout: Duration) -> Option<Vec<PathBuf>> {
    let mut paths = vec![rx.recv_timeout(timeout).ok()?];
    while let Ok(path) = rx.recv_timeout(Duration::from_millis(150)) {
        paths.push(path);
    }
    Some(paths)
}

/// The next batch `Notify` sends, waiting up to `timeout`.
fn next_batch(rx: &mpsc::Receiver<Vec<PathBuf>>, timeout: Duration) -> Option<Vec<PathBuf>> {
    rx.recv_timeout(timeout).ok()
}

// ── file modification triggers recompilation ──────────────────

/// A modified file is reported as its whole path under the (canonical)
/// watched directory: what it is to the project is the session's to say.
#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "file modification triggers recompilation"
)]
fn changed_paths_are_reported_whole() {
    let dir = TempDir::new().unwrap();
    let spec_path = dir.path().join("a.spec");
    fs::write(&spec_path, r#"behavior foo "Foo" { contract "x" }"#).unwrap();

    let (tx, rx) = mpsc::channel();
    let _watcher = SpecWatcher::new(dir.path(), tx).unwrap();

    // Give watcher time to start
    std::thread::sleep(Duration::from_millis(100));

    // Modify the file
    fs::write(&spec_path, r#"behavior bar "Bar" { contract "y" }"#).unwrap();

    let event = wait_for_event(&rx, Duration::from_secs(2));
    assert!(
        event.is_some(),
        "should receive change event after file modification"
    );
    let events = event.unwrap();
    let expected = fs::canonicalize(dir.path()).unwrap().join("a.spec");
    assert!(
        events.contains(&expected),
        "changed paths should include {expected:?}, got: {events:?}"
    );
    assert!(events.iter().all(|p| p.is_absolute()), "{events:?}");
}

// ── file creation triggers recompilation ──────────────────────

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "file creation triggers recompilation"
)]
fn file_creation_triggers_recompilation() {
    let dir = TempDir::new().unwrap();

    let (tx, rx) = mpsc::channel();
    let _watcher = SpecWatcher::new(dir.path(), tx).unwrap();

    std::thread::sleep(Duration::from_millis(100));

    // Create a new spec file
    let new_path = dir.path().join("new.spec");
    fs::write(&new_path, r#"behavior new_thing "New" { contract "z" }"#).unwrap();

    let event = wait_for_event(&rx, Duration::from_secs(2));
    assert!(
        event.is_some(),
        "should receive change event after file creation"
    );
    let events = event.unwrap();
    assert!(
        events.iter().any(|p| p.ends_with("new.spec")),
        "changed files should include new.spec, got: {:?}",
        events
    );
}

// ── file deletion triggers recompilation ──────────────────────

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "file deletion triggers recompilation"
)]
fn file_deletion_triggers_recompilation() {
    let dir = TempDir::new().unwrap();
    let spec_path = dir.path().join("doomed.spec");
    fs::write(&spec_path, r#"behavior doomed "Doomed" { contract "bye" }"#).unwrap();

    let (tx, rx) = mpsc::channel();
    let _watcher = SpecWatcher::new(dir.path(), tx).unwrap();

    std::thread::sleep(Duration::from_millis(100));

    // Delete the file
    fs::remove_file(&spec_path).unwrap();

    let event = wait_for_event(&rx, Duration::from_secs(2));
    assert!(
        event.is_some(),
        "should receive change event after file deletion"
    );
    let events = event.unwrap();
    assert!(
        events.iter().any(|p| p.ends_with("doomed.spec")),
        "changed files should include doomed.spec, got: {:?}",
        events
    );
}

// ── latency integration test ──────────────────────────────────

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "watch detects changes within 100ms"
)]
fn watch_detects_changes_within_latency_target() {
    use specforge_project::WatchRoot;
    use specforge_watch::{Notify, Watchers};

    let dir = TempDir::new().unwrap();
    let spec_path = dir.path().join("latency.spec");
    fs::write(&spec_path, r#"behavior init "Init" { contract "x" }"#).unwrap();

    // A short debounce window so the measurement is dominated by detection,
    // not by coalescing.
    let debounce = Duration::from_millis(10);
    let (mut notify, batches) = Notify::new(debounce);
    notify
        .watch(&[WatchRoot {
            dir: dir.path().to_path_buf(),
            recursive: true,
        }])
        .unwrap();

    std::thread::sleep(Duration::from_millis(200));

    let start = Instant::now();
    fs::write(&spec_path, r#"behavior updated "Updated" { contract "y" }"#).unwrap();

    let event = next_batch(&batches, Duration::from_secs(2));
    let elapsed = start.elapsed();

    let events = event.expect("should receive change event");
    assert!(
        events.iter().any(|p| p.ends_with("latency.spec")),
        "{events:?}"
    );
    // Obligation: detected within 100ms. The batch is emitted after the
    // debounce window of silence, so the bound is 100ms + that window.
    let bound = Duration::from_millis(100) + debounce;
    assert!(
        elapsed < bound,
        "change detection took {elapsed:?}, expected < {bound:?}"
    );
}

// ── contract test ─────────────────────────────────────────────

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "Watch File System for Changes: file system watching holds for the declared obligations"
)]
fn watch_contract_consistency() {
    let dir = TempDir::new().unwrap();

    // Requires: watch mode active on spec root
    let (tx, rx) = mpsc::channel();
    let _watcher = SpecWatcher::new(dir.path(), tx).unwrap();
    std::thread::sleep(Duration::from_millis(200));

    // Ensures: file_changed event produced for creation
    let spec_path = dir.path().join("contract.spec");
    fs::write(&spec_path, r#"behavior a "A" { contract "x" }"#).unwrap();
    let event = wait_for_event(&rx, Duration::from_secs(2));
    assert!(event.is_some(), "ensures: file_changed for creation");

    // Ensures: file_changed event produced for modification
    fs::write(&spec_path, r#"behavior b "B" { contract "y" }"#).unwrap();
    let event = wait_for_event(&rx, Duration::from_secs(2));
    assert!(event.is_some(), "ensures: file_changed for modification");

    // Any other file is reported too, whole: the project session decides
    // that it changes nothing (classify_project_changes), so watch does
    // not recompile for it.
    let txt_path = dir.path().join("readme.txt");
    fs::write(&txt_path, "not a spec").unwrap();
    let event = wait_for_event(&rx, Duration::from_secs(2)).expect("readme.txt reported");
    assert!(event.iter().any(|p| p.ends_with("readme.txt")), "{event:?}");
}

// ── a shallow watcher ─────────────────────────────────────────

/// The nearest existing ancestor of a missing directory is watched for its
/// own entries only: the next directory on the way is reported, what is
/// below an existing one is not.
#[test]
fn a_shallow_watcher_reports_its_own_entries_only() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("sub")).unwrap();
    let (tx, rx) = mpsc::channel();
    let _watcher = SpecWatcher::shallow(dir.path(), tx).unwrap();
    // macOS can deliver events for the directory's own creation late.
    std::thread::sleep(Duration::from_millis(1000));
    while wait_for_event(&rx, Duration::from_millis(300)).is_some() {}

    // Below an entry: not reported.
    fs::write(dir.path().join("sub/deep.md"), "deep").unwrap();
    let below = wait_for_event(&rx, Duration::from_millis(800));
    assert!(
        below.is_none(),
        "a change below the directory was reported: {below:?}"
    );

    // Its own entry: reported, whole.
    fs::write(dir.path().join("own.md"), "own").unwrap();
    let events = wait_for_event(&rx, Duration::from_secs(5)).expect("its own entry is reported");
    let expected = fs::canonicalize(dir.path()).unwrap().join("own.md");
    assert!(events.contains(&expected), "{events:?}");
}

// ── one burst across two watched roots ────────────────────────

#[spec(
    behavior = "debounce_file_changes",
    verify = "changes under different watched directories within the window join one batch"
)]
fn a_burst_across_two_roots_is_one_batch() {
    use specforge_project::WatchRoot;
    use specforge_watch::{Notify, Watchers};

    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let (mut notify, batches) = Notify::new(Duration::from_millis(50));
    notify
        .watch(&[
            WatchRoot {
                dir: a.path().to_path_buf(),
                recursive: true,
            },
            WatchRoot {
                dir: b.path().to_path_buf(),
                recursive: true,
            },
        ])
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    fs::write(a.path().join("x.spec"), "a").unwrap();
    fs::write(b.path().join("y.spec"), "b").unwrap();

    let mut seen = Vec::new();
    while let Some(batch) = next_batch(&batches, Duration::from_secs(1)) {
        seen.push(batch);
    }
    let x = fs::canonicalize(a.path()).unwrap().join("x.spec");
    let y = fs::canonicalize(b.path()).unwrap().join("y.spec");
    let mut expected = vec![x, y];
    expected.sort();
    assert_eq!(seen, [expected]);
}
