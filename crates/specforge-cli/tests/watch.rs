// `specforge watch` — end-to-end smoke test against the real binary.

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use assert_cmd::cargo::CommandCargoExt;
use specforge_test::prelude::*;
use tempfile::TempDir;

use crate::child_guard::{ChildGuard, guarded_command};

#[allow(deprecated)]
fn specforge_cmd() -> Command {
    Command::cargo_bin("specforge").unwrap()
}

/// The `delta` of a `rebuilt` or `rechecked` line: the graph delta MCP
/// notifies.
fn delta_of(line: &str) -> serde_json::Value {
    let event: serde_json::Value =
        serde_json::from_str(line).unwrap_or_else(|e| panic!("a watch line is JSON ({e}): {line}"));
    event["delta"].clone()
}

/// Spawn `specforge watch --json` on a fresh project and return the line
/// receiver plus the child handle.
fn spawn_watch(project: &TempDir) -> (mpsc::Receiver<String>, ChildGuard) {
    spawn_watch_with(project, &[])
}

fn spawn_watch_with(project: &TempDir, extra: &[&str]) -> (mpsc::Receiver<String>, ChildGuard) {
    let mut watch = specforge_cmd();
    watch
        .args([
            "watch",
            "--path",
            project.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .args(extra);
    let mut child = ChildGuard::spawn(
        guarded_command(&watch)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .expect("failed to spawn specforge watch");

    let stdout = child.take_stdout().expect("stdout piped");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    (rx, child)
}

/// Wait for a line matching `needle` within `timeout`.
fn wait_for_line(rx: &mpsc::Receiver<String>, needle: &str, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(line) => {
                if line.contains(needle) {
                    return Some(line);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
    None
}

#[test]
fn watch_rebuilds_on_file_change() {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("specforge.json"), "{}").unwrap();
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\n",
    )
    .unwrap();

    let (rx, child) = spawn_watch(&project);

    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60));
    assert!(ready.is_some(), "watch never reported ready");

    // Give the watcher a moment to settle, then grow the spec file.
    std::thread::sleep(Duration::from_millis(300));
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\nentity two { title \"Two\" }\n",
    )
    .unwrap();

    // A late event for the project's initial write can still arrive after
    // `ready` (macOS coalesces file events) and rebuild unchanged content;
    // the rebuild for this edit is the one that adds a node.
    let rebuilt =
        wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60)).and_then(|first| {
            let delta = delta_of(&first);
            if delta["added_nodes"] == serde_json::json!([])
                && delta["modified_nodes"] == serde_json::json!([])
            {
                wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60))
            } else {
                Some(first)
            }
        });
    drop(child);

    let line = rebuilt.expect("no rebuild event after file change");
    assert_eq!(
        delta_of(&line)["added_nodes"].as_array().map(Vec::len),
        Some(1),
        "expected exactly one added node, got: {line}"
    );
    assert!(
        line.contains("\"errors\":0"),
        "clean edit must stay diagnostic-free: {line}"
    );
}

#[test]
fn watch_reports_diagnostics_on_broken_edit() {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("specforge.json"), "{}").unwrap();
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\n",
    )
    .unwrap();

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60));
    assert!(ready.is_some(), "watch never reported ready");

    std::thread::sleep(Duration::from_millis(300));
    // Reference a nonexistent entity -> E003 unresolved reference.
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\"\n  uses [ghost]\n}\n",
    )
    .unwrap();

    // As in watch_rebuilds_on_file_change: a late event for the initial
    // write can rebuild unchanged content first; the rebuild for this edit
    // is the one that modifies `one` and reports the error.
    let rebuilt =
        wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60)).and_then(|first| {
            if first.contains("\"errors\":0")
                && delta_of(&first)["modified_nodes"] == serde_json::json!([])
            {
                wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60))
            } else {
                Some(first)
            }
        });
    drop(child);

    let line = rebuilt.expect("no rebuild event after file change");
    assert!(
        line.contains("\"errors\":1"),
        "broken edit must surface one error: {line}"
    );
    assert!(
        line.contains("main.spec"),
        "changed diagnostic files must include the edited file: {line}"
    );
}

#[test]
fn watch_verify_incremental_checks_each_rebuild_against_a_cold_one() {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("specforge.json"), "{}").unwrap();
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\n",
    )
    .unwrap();

    let (rx, child) = spawn_watch_with(&project, &["--verify-incremental"]);
    assert!(
        wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60)).is_some(),
        "watch never reported ready"
    );
    std::thread::sleep(Duration::from_millis(300));
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\nentity two { title \"Two\" }\n",
    )
    .unwrap();

    let rebuilt = wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60));
    drop(child);

    let line = rebuilt.expect("no rebuild event after file change");
    let event: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(event["verification"], "passed", "{line}");
}

// The debug build of the compiler checks every rebuild; a release build only
// with --verify-incremental. The binary is built in the test's profile, so
// this test exists only in debug builds.
#[cfg(debug_assertions)]
#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "a debug build checks each rebuild without the flag"
)]
fn watch_debug_build_verifies_without_the_flag() {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("specforge.json"), "{}").unwrap();
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\n",
    )
    .unwrap();

    let (rx, child) = spawn_watch(&project);
    assert!(
        wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60)).is_some(),
        "watch never reported ready"
    );
    std::thread::sleep(Duration::from_millis(300));
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\nentity two { title \"Two\" }\n",
    )
    .unwrap();

    let rebuilt = wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60));
    drop(child);

    let line = rebuilt.expect("no rebuild event after file change");
    let event: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(event["verification"], "passed", "{line}");
}

#[test]
fn watch_rebuild_reports_what_check_reports() {
    let project = TempDir::new().unwrap();
    fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"w","version":"0.1.0","extensions":["@specforge/software","@specforge/testing"]}"#,
    )
    .unwrap();
    fs::write(
        project.path().join("main.spec"),
        "behavior login \"L\" {\n  category command\n  contract \"c\"\n  verify unit \"v\"\n}\n",
    )
    .unwrap();

    let (rx, child) = spawn_watch(&project);
    assert!(
        wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60)).is_some(),
        "watch never reported ready"
    );
    std::thread::sleep(Duration::from_millis(300));
    // Drop the required contract: software's rule reports E006.
    let broken = "behavior login \"L\" {\n  category command\n  verify unit \"v\"\n}\n";
    fs::write(project.path().join("main.spec"), broken).unwrap();

    let rebuilt = wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60));
    drop(child);

    let check = specforge_cmd()
        .args(["check", "--format", "json"])
        .arg("--path")
        .arg(project.path())
        .output()
        .unwrap();
    let check: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    let check_errors = check
        .as_array()
        .unwrap_or_else(|| panic!("{check}"))
        .iter()
        .filter(|d| {
            d["severity"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("error"))
        })
        .count();
    assert!(
        check_errors >= 1,
        "check sees the missing contract: {check}"
    );

    let event: serde_json::Value = serde_json::from_str(&rebuilt.expect("no rebuild")).unwrap();
    assert_eq!(
        event["errors"], check_errors,
        "watch: {event}\ncheck: {check}"
    );
}

/// A diagnostic list (check's JSON, or a watch event's `diagnostics`) as a
/// multiset of compact JSON: order-independent, nothing dropped.
fn diagnostic_set(list: &serde_json::Value) -> std::collections::BTreeMap<String, usize> {
    let mut set = std::collections::BTreeMap::new();
    for d in list
        .as_array()
        .unwrap_or_else(|| panic!("not a list: {list}"))
    {
        *set.entry(d.to_string()).or_default() += 1;
    }
    set
}

/// Copy a parity fixture, run watch on it until `ready`, rewrite `main.spec`
/// unchanged plus a newline, and return (ready, rebuilt, check's list).
fn watch_and_check(fixture: &str) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let project = TempDir::new().unwrap();
    let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/parity")
        .join(fixture);
    for entry in fs::read_dir(&from).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), project.path().join(entry.file_name())).unwrap();
    }

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    std::thread::sleep(Duration::from_millis(300));
    let main = project.path().join("main.spec");
    let text = fs::read_to_string(&main).unwrap();
    fs::write(&main, format!("{text}\n")).unwrap();
    let rebuilt = wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60))
        .expect("no rebuild event");
    drop(child);

    let check = specforge_cmd()
        .args(["check", "--format", "json"])
        .arg("--path")
        .arg(project.path())
        .output()
        .unwrap();
    (
        serde_json::from_str(&ready).unwrap(),
        serde_json::from_str(&rebuilt).unwrap(),
        serde_json::from_slice(&check.stdout).unwrap(),
    )
}

/// The resolver's E025 reaches watch at startup and after a rebuild, with
/// everything else `check` reports (plan 01, D1).
#[specforge_test(
    behavior = "emit_incremental_diagnostics",
    verify = "total diagnostic set matches full rebuild"
)]
fn watch_reports_what_check_reports_for_a_missing_import() {
    let (ready, rebuilt, check) = watch_and_check("missing_import");
    assert!(
        diagnostic_set(&check)
            .keys()
            .any(|d| d.contains("\"E025\"")),
        "{check}"
    );
    assert_eq!(
        diagnostic_set(&ready["diagnostics"]),
        diagnostic_set(&check)
    );
    assert_eq!(
        diagnostic_set(&rebuilt["diagnostics"]),
        diagnostic_set(&check)
    );
    assert_eq!(rebuilt["verification"], "passed", "{rebuilt}");
}

/// An extension that fails to load (E028) is reported by watch as by
/// `check`, before and after a rebuild (plan 01, D2).
#[specforge_test(
    behavior = "rebuild_affected_subgraph",
    verify = "incremental rebuild equals cold rebuild"
)]
fn watch_reports_what_check_reports_for_an_extension_that_fails_to_load() {
    let (ready, rebuilt, check) = watch_and_check("unknown_extension");
    assert!(
        diagnostic_set(&check)
            .keys()
            .any(|d| d.contains("\"E028\"")),
        "{check}"
    );
    assert_eq!(
        diagnostic_set(&ready["diagnostics"]),
        diagnostic_set(&check)
    );
    assert_eq!(
        diagnostic_set(&rebuilt["diagnostics"]),
        diagnostic_set(&check)
    );
    assert_eq!(rebuilt["verification"], "passed", "{rebuilt}");
}

/// The next JSON event line, skipping a late `rebuilt` for content that
/// did not change (macOS can deliver an event for a project's initial
/// write after `ready`).
fn next_event(rx: &mpsc::Receiver<String>, timeout: Duration) -> Option<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let unchanged = event["event"] == "rebuilt"
            && event["delta"]["added_nodes"] == serde_json::json!([])
            && event["delta"]["removed_nodes"] == serde_json::json!([])
            && event["delta"]["modified_nodes"] == serde_json::json!([]);
        if !unchanged {
            return Some(event);
        }
    }
    None
}

/// Let the watcher settle after `ready`: macOS can deliver events for the
/// project's initial writes late (a late config event even reloads), so
/// they are drained before a test makes the change it watches for.
fn settle(rx: &mpsc::Receiver<String>) {
    std::thread::sleep(Duration::from_millis(1000));
    while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
}

/// The E028 messages an event reports.
fn e028(event: &serde_json::Value) -> Vec<String> {
    event["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E028")
        .map(|d| d["message"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "watch_file_system_for_changes",
    verify = "a specforge.lock change reloads the environment"
)]
fn watch_reloads_on_a_lock_change() {
    let project = TempDir::new().unwrap();
    fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"w","version":"0.1.0","extensions":["@acme/missing"]}"#,
    )
    .unwrap();
    fs::write(project.path().join("main.spec"), "").unwrap();

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    let unlocked = e028(&ready);
    assert!(
        unlocked
            .iter()
            .any(|m| m.contains("no specforge.lock entry")),
        "{unlocked:?}"
    );

    settle(&rx);
    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": [{"name": "@acme/missing", "version": "1.0.0", "source": "local:missing.wasm", "wasm_hash": "sha256:00"}]
    });
    fs::write(project.path().join("specforge.lock"), lock.to_string()).unwrap();

    let event = next_event(&rx, Duration::from_secs(20));
    drop(child);
    let event = event.expect("no event after the lock change");
    assert_eq!(event["event"], "extensions_reloaded", "{event}");
    let locked = e028(&event);
    assert_eq!(locked.len(), 1, "{event}");
    assert_ne!(
        locked, unlocked,
        "the lock changed what the environment loads"
    );
}

#[specforge_test(
    behavior = "watch_file_system_for_changes",
    verify = "a .wasm file no extension loads changes nothing"
)]
fn watch_ignores_a_wasm_no_extension_loads() {
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("specforge.json"), "{}").unwrap();
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\n",
    )
    .unwrap();

    let (rx, child) = spawn_watch(&project);
    wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    settle(&rx);

    // Build output no extension loads, then a real edit: the first event
    // is the edit's rebuild, not a reload for the .wasm.
    fs::create_dir_all(project.path().join("target")).unwrap();
    fs::write(project.path().join("target/x.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\nentity two { title \"Two\" }\n",
    )
    .unwrap();

    let event = next_event(&rx, Duration::from_secs(20));
    drop(child);
    let event = event.expect("no event after the edit");
    assert_eq!(event["event"], "rebuilt", "{event}");
    assert_eq!(
        event["changed"],
        serde_json::json!(["main.spec"]),
        "{event}"
    );
    assert_eq!(
        event["delta"]["added_nodes"].as_array().map(Vec::len),
        Some(1),
        "{event}"
    );
}

#[specforge_test(
    behavior = "watch_file_system_for_changes",
    verify = "after spec_root changes, files under the new spec root are watched"
)]
fn watch_follows_a_moved_spec_root() {
    let project = TempDir::new().unwrap();
    let root = project.path();
    fs::write(
        root.join("specforge.json"),
        r#"{"name":"w","version":"0.1.0","spec_root":"a"}"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/one.spec"), "entity one { title \"One\" }\n").unwrap();

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    assert!(ready.contains("\"nodes\":1"), "{ready}");
    settle(&rx);

    fs::write(
        root.join("specforge.json"),
        r#"{"name":"w","version":"0.1.0","spec_root":"b"}"#,
    )
    .unwrap();
    let reloaded = next_event(&rx, Duration::from_secs(20)).expect("no reload");
    assert_eq!(reloaded["event"], "extensions_reloaded", "{reloaded}");
    assert_eq!(reloaded["nodes"], 0, "{reloaded}");
    std::thread::sleep(Duration::from_millis(300));

    // The old spec root is no longer the project's: an edit there changes
    // nothing; a file under the new one is a source.
    fs::write(
        root.join("a/one.spec"),
        "entity one { title \"One\" }\nentity uno { title \"Uno\" }\n",
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    fs::write(
        root.join("b/three.spec"),
        "entity three { title \"Three\" }\n",
    )
    .unwrap();

    let event = next_event(&rx, Duration::from_secs(20));
    drop(child);
    let event = event.expect("no event for the new spec root");
    assert_eq!(event["event"], "rebuilt", "{event}");
    assert_eq!(
        event["rebuilt_files"],
        serde_json::json!(["three.spec"]),
        "{event}"
    );
    assert_eq!(
        event["delta"]["added_nodes"].as_array().map(Vec::len),
        Some(1),
        "{event}"
    );
}

/// A project whose `gadget` kind (the vendored docref extension) has a
/// `docs` field naming files, with `spec/a.spec` holding `spec`. The
/// extension is a `.wasm` entry copied into `ext/`.
fn docref_project(spec: &str) -> TempDir {
    let project = TempDir::new().unwrap();
    let root = project.path();
    fs::create_dir_all(root.join("spec")).unwrap();
    fs::create_dir_all(root.join("ext")).unwrap();
    fs::write(
        root.join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@sdk/docref=ext/docref.wasm"]}"#,
    )
    .unwrap();
    fs::copy(
        specforge_component::committed::DOCREF.path(),
        root.join("ext/docref.wasm"),
    )
    .unwrap();
    fs::write(root.join("spec/a.spec"), spec).unwrap();
    project
}

/// A `gadget` whose `docs` names `reference`.
fn gadget_naming(reference: &str) -> String {
    format!("gadget gadget_one \"G\" {{\n  docs [\"{reference}\"]\n}}\n")
}

/// How `spec/a.spec` of a `docref_project` spells a file in `outside/<rest>`,
/// a sibling temp directory: from `spec/`, two levels up is their parent.
fn outside_reference(outside: &TempDir, rest: &str) -> String {
    let name = outside.path().file_name().unwrap().to_string_lossy();
    format!("../../{name}/{rest}")
}

/// The diagnostic codes an event reports.
fn codes(event: &serde_json::Value) -> Vec<String> {
    event["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "classify_project_changes",
    verify = "a file a file_reference field names re-runs the checks"
)]
fn watch_rechecks_when_a_referenced_file_outside_the_root_appears() {
    let outside = TempDir::new().unwrap();
    fs::create_dir_all(outside.path().join("shared")).unwrap();
    let project = docref_project(&gadget_naming(&outside_reference(
        &outside,
        "shared/guide.md",
    )));

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    assert!(codes(&ready).contains(&"E016".to_string()), "{ready}");
    settle(&rx);

    fs::write(outside.path().join("shared/guide.md"), "# guide\n").unwrap();
    let event = next_event(&rx, Duration::from_secs(20));
    drop(child);
    let event = event.expect("no event after the referenced file appeared");
    assert_eq!(event["event"], "rechecked", "{event}");
    assert!(!codes(&event).contains(&"E016".to_string()), "{event}");
    let changed = event["changed"].as_array().unwrap();
    assert_eq!(changed.len(), 1, "{event}");
    assert!(
        changed[0].as_str().unwrap().ends_with("shared/guide.md"),
        "{event}"
    );
}

#[specforge_test(
    behavior = "watch_file_system_for_changes",
    verify = "after an edit names a file outside the watched directories, a change to it is seen"
)]
fn watch_follows_a_file_an_edit_names_outside_its_roots() {
    let outside = TempDir::new().unwrap();
    fs::create_dir_all(outside.path().join("shared")).unwrap();
    let project = docref_project("gadget gadget_one \"G\" {\n}\n");

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    assert!(!codes(&ready).contains(&"E016".to_string()), "{ready}");
    settle(&rx);

    fs::write(
        project.path().join("spec/a.spec"),
        gadget_naming(&outside_reference(&outside, "shared/guide.md")),
    )
    .unwrap();
    let edited = next_event(&rx, Duration::from_secs(20)).expect("no rebuild after the edit");
    assert_eq!(edited["event"], "rebuilt", "{edited}");
    assert!(codes(&edited).contains(&"E016".to_string()), "{edited}");
    settle(&rx);

    fs::write(outside.path().join("shared/guide.md"), "# guide\n").unwrap();
    let event = next_event(&rx, Duration::from_secs(20));
    drop(child);
    let event = event.expect("no event after the file the edit named appeared");
    assert_eq!(event["event"], "rechecked", "{event}");
    assert_eq!(event["errors"], 0, "{event}");
    assert!(!codes(&event).contains(&"E016".to_string()), "{event}");
    let changed = event["changed"].as_array().unwrap();
    assert_eq!(changed.len(), 1, "{event}");
    assert!(
        changed[0].as_str().unwrap().ends_with("shared/guide.md"),
        "{event}"
    );
}

#[specforge_test(
    behavior = "watch_file_system_for_changes",
    verify = "a file the checks read is seen when it is created in a directory that did not exist"
)]
fn watch_sees_a_referenced_file_created_in_a_new_directory() {
    let outside = TempDir::new().unwrap();
    let project = docref_project(&gadget_naming(&outside_reference(
        &outside,
        "nodir/guide.md",
    )));

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    assert!(codes(&ready).contains(&"E016".to_string()), "{ready}");
    settle(&rx);

    fs::create_dir_all(outside.path().join("nodir")).unwrap();
    fs::write(outside.path().join("nodir/guide.md"), "# guide\n").unwrap();

    // The directory's creation is a check input's change; the file may be
    // seen with it or after the watcher moved into the new directory. The
    // last event clears E016.
    let mut last =
        next_event(&rx, Duration::from_secs(20)).expect("no event for the new directory");
    while let Some(next) = next_event(&rx, Duration::from_secs(3)) {
        last = next;
    }
    drop(child);
    assert_eq!(last["event"], "rechecked", "{last}");
    assert_eq!(last["errors"], 0, "{last}");
    assert!(!codes(&last).contains(&"E016".to_string()), "{last}");
}

/// A reader that leaves ends the watch (`specforge watch | head -1`): the
/// next event it would write finds stdout closed, and the command ends with
/// its own exit, 0.
#[cfg(unix)]
#[test]
fn watch_ends_when_its_reader_leaves() {
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let project = TempDir::new().unwrap();
    fs::write(project.path().join("specforge.json"), "{}").unwrap();
    fs::write(
        project.path().join("main.spec"),
        "entity one { title \"One\" }\n",
    )
    .unwrap();

    let mut child = Kill(
        specforge_cmd()
            .args([
                "watch",
                "--format",
                "json",
                "--path",
                project.path().to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .expect("failed to spawn specforge watch"),
    );
    let mut stdout = BufReader::new(child.0.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        let read = stdout.read_line(&mut line).expect("watch's stdout");
        assert!(read > 0, "watch ended before it reported ready");
        if line.contains("\"event\":\"ready\"") {
            break;
        }
    }
    // The reader leaves.
    drop(stdout);

    // Every change makes the watch write an event into the closed pipe.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut edits = 1;
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "watch kept running after its reader left"
        );
        let mut spec = String::from("entity one { title \"One\" }\n");
        for n in 0..edits {
            spec.push_str(&format!("entity extra{n} {{ title \"Extra\" }}\n"));
        }
        fs::write(project.path().join("main.spec"), spec).unwrap();
        edits += 1;
        std::thread::sleep(Duration::from_millis(500));
    };
    assert_eq!(status.code(), Some(0), "the command's own exit");
}

/// A file discovery does not find (an excluded `drafts/x.spec`) is no
/// source: creating it moves nothing, and every event watch sends is what
/// `check` reports (ADR 0055).
#[specforge_test(
    invariant = "incremental_correctness",
    verify = "a file discovery does not find changes nothing a compile or a session reports"
)]
fn watch_reports_what_check_reports_when_an_excluded_import_target_appears() {
    let project = TempDir::new().unwrap();
    fs::write(
        project.path().join("specforge.json"),
        r#"{"name":"w","version":"0.1.0","extensions":[],"exclude":["drafts/"]}"#,
    )
    .unwrap();
    fs::write(
        project.path().join("a.spec"),
        "use \"drafts/x\"\n\nterm a \"A\" {\n}\n",
    )
    .unwrap();
    fs::write(project.path().join("b.spec"), "term b \"B\" {\n}\n").unwrap();
    let check = |project: &TempDir| -> serde_json::Value {
        let out = specforge_cmd()
            .args(["check", "--format", "json"])
            .arg("--path")
            .arg(project.path())
            .output()
            .unwrap();
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let (rx, child) = spawn_watch(&project);
    let ready = wait_for_line(&rx, "\"event\":\"ready\"", Duration::from_secs(60))
        .expect("watch never reported ready");
    settle(&rx);
    let mut last: serde_json::Value = serde_json::from_str(&ready).unwrap();

    fs::create_dir_all(project.path().join("drafts")).unwrap();
    fs::write(project.path().join("drafts/x.spec"), "term x \"X\" {\n}\n").unwrap();
    while let Some(event) = next_event(&rx, Duration::from_secs(2)) {
        last = event;
    }
    let checked = check(&project);
    assert!(
        diagnostic_set(&checked)
            .keys()
            .any(|d| d.contains("\"E025\"")),
        "{checked}"
    );
    assert_eq!(
        diagnostic_set(&last["diagnostics"]),
        diagnostic_set(&checked)
    );

    fs::write(project.path().join("b.spec"), "term b \"B2\" {\n}\n").unwrap();
    let rebuilt = wait_for_line(&rx, "\"event\":\"rebuilt\"", Duration::from_secs(60))
        .expect("no rebuild event");
    drop(child);
    let rebuilt: serde_json::Value = serde_json::from_str(&rebuilt).unwrap();
    assert_eq!(
        diagnostic_set(&rebuilt["diagnostics"]),
        diagnostic_set(&check(&project))
    );
}
