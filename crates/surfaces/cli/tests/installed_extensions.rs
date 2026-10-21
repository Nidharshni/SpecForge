//! Installed (non-builtin) extensions load from the lock file (plan 03,
//! O4.4, ADR 0004 D3-b). Until this, no third-party extension had ever
//! loaded through `check`: the loader keyed modules by file stem while
//! compile looked them up by name, and nothing read the lock.

use crate::support::registry::greet_wasm;
use crate::support::registry::project_on;
use crate::support::registry::{Package, serve};
use crate::support::{Cli, TestProject};
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;
use std::path::Path;

/// A spec using `greeting`, the kind only `@sdk/greet` defines.
const GREETING: &str = "greeting hello \"Hello\" {\n  style warm\n}\n";

/// A project enabling `@specforge/software` with a spec that uses the
/// `greeting` kind.
fn greeting_project() -> TestProject {
    TestProject::new()
        .config(|c| c["name"] = "p".into())
        .enabling(&["@specforge/software"])
        .file("main.spec", GREETING)
}

fn add_local_greet(project: &TestProject) {
    project.write("greet.wasm", greet_wasm());
    Cli::on(project)
        .command(&["add", "greet.wasm"])
        .assert()
        .success();
}

/// Every diagnostic a fresh `specforge check` reports, as `(code, message,
/// suggestion)`, with its exit status.
fn check(project: &TestProject) -> (bool, Vec<(String, String, String)>) {
    let out = Cli::on(project)
        .command(&["check", "--format", "json"])
        .output()
        .unwrap();
    let diagnostics: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let found = diagnostics
        .iter()
        .map(|d| {
            let text = |key: &str| d[key].as_str().unwrap_or_default().to_string();
            (text("code"), text("message"), text("suggestion"))
        })
        .collect();
    (out.status.success(), found)
}

fn codes(found: &[(String, String, String)]) -> Vec<&str> {
    found.iter().map(|(code, _, _)| code.as_str()).collect()
}

fn lock_entry(root: &Path, name: &str) -> Value {
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("specforge.lock")).unwrap())
            .unwrap();
    lock["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no lock entry for {name}: {lock}"))
}

fn enabled(root: &Path) -> Value {
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("specforge.json")).unwrap())
            .unwrap();
    config["extensions"].clone()
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "installed extension manifest is loaded"
)]
fn a_local_install_loads_through_check() {
    let dir = greeting_project();
    let (_, before) = check(&dir);
    assert!(codes(&before).contains(&"E024"), "{before:?}");

    add_local_greet(&dir);

    let (ok, after) = check(&dir);
    assert!(ok, "{after:?}");
    // `greeting` is a known kind now, and nothing failed to load — its
    // field types (W019) and rule (W112) included.
    for code in ["E024", "E028", "E033", "E070", "W019", "W112"] {
        assert!(!codes(&after).contains(&code), "{code}: {after:?}");
    }
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a local .wasm install is locked at its declared version with source local:<path> and enabled by its bare name"
)]
fn a_local_install_is_locked_at_its_declared_version() {
    let dir = greeting_project();

    add_local_greet(&dir);

    // The handshake's name and version, not the file stem or a placeholder.
    let entry = lock_entry(dir.root(), "@sdk/greet");
    assert_eq!(entry["version"], "0.1.0", "{entry}");
    assert_eq!(entry["source"], "local:greet.wasm", "{entry}");
    assert_eq!(
        enabled(dir.root()),
        json!(["@specforge/software", "@sdk/greet"])
    );
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "an extension installed from a registry loads through check"
)]
fn a_registry_install_loads_through_check() {
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_on(&registry);
    dir.write("main.spec", GREETING);

    Cli::on(&dir)
        .command(&["add", "@sdk/greet@0.1.0", "--allow-unsigned"])
        .assert()
        .success();

    assert_eq!(lock_entry(dir.root(), "@sdk/greet")["source"], "registry");
    assert_eq!(
        enabled(dir.root()),
        json!(["@specforge/software", "@sdk/greet"])
    );
    let (ok, found) = check(&dir);
    assert!(ok, "{found:?}");
    assert!(!codes(&found).contains(&"E024"), "{found:?}");
}

/// The JSON `specforge add` always names its source, and a key only when the
/// package is signed.
#[test]
fn add_json_names_its_source_and_a_key_only_when_signed() {
    let dir = greeting_project();
    dir.write("greet.wasm", greet_wasm());
    let out = Cli::on(&dir)
        .command(&["add", "greet.wasm", "--format", "json"])
        .current_dir(dir.root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let local: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(local["added"][0]["source"], "local:greet.wasm");
    assert!(local["added"][0].get("key_id").is_none(), "{local}");

    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_on(&registry);
    let out = Cli::on(&dir)
        .command(&[
            "add",
            "@sdk/greet@0.1.0",
            "--allow-unsigned",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let remote: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(remote["added"][0].get("key_id").is_none(), "{remote}");
    assert_eq!(remote["added"][0]["source"], "registry", "{remote}");
    assert_eq!(remote["added"][0]["publisher"], "unsigned", "{remote}");
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "tampered installed binary refused via lockfile hash pin (E070)"
)]
fn a_tampered_installed_binary_is_refused_with_e070() {
    let dir = greeting_project();
    add_local_greet(&dir);
    let installed = dir
        .root()
        .join(".specforge/extensions/@sdk/greet/extension.wasm");
    let mut bytes = std::fs::read(&installed).unwrap();
    bytes.extend_from_slice(b"tampered");
    std::fs::write(&installed, bytes).unwrap();

    let (ok, found) = check(&dir);

    assert!(!ok, "{found:?}");
    let (_, message, _) = found
        .iter()
        .find(|(code, _, _)| code == "E070")
        .unwrap_or_else(|| panic!("no E070: {found:?}"));
    assert!(message.contains("@sdk/greet"), "{message}");
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "an enabled extension with no installed binary produces E028 naming the command that installs it"
)]
fn an_enabled_extension_that_is_not_installed_is_e028_with_a_remedy() {
    let dir = greeting_project();
    add_local_greet(&dir);
    // A fresh clone: `.specforge/` is gitignored, the config and lock are not.
    std::fs::remove_dir_all(dir.root().join(".specforge")).unwrap();

    let (ok, found) = check(&dir);

    assert!(!ok, "{found:?}");
    let (_, message, suggestion) = found
        .iter()
        .find(|(code, _, _)| code == "E028")
        .unwrap_or_else(|| panic!("no E028: {found:?}"));
    assert!(message.contains("@sdk/greet"), "{message}");
    assert!(suggestion.contains("specforge add"), "{suggestion}");
}

#[specforge_test(
    invariant = "init_config_validity",
    verify = "specforge init followed by specforge check produces zero config errors"
)]
fn init_installs_a_local_extension_check_loads() {
    let dir = TestProject::new()
        .without_config()
        .file("greet.wasm", greet_wasm());

    // D3-e: init installs a local file through the add operation.
    Cli::on(&dir)
        .bare_command(&["init", "--name", "hello", "--extensions", "greet.wasm"])
        .assert()
        .success();
    std::fs::write(dir.root().join("spec/greet.spec"), GREETING).unwrap();

    assert_eq!(enabled(dir.root()), json!(["@sdk/greet"]));
    assert_eq!(
        lock_entry(dir.root(), "@sdk/greet")["source"],
        "local:greet.wasm"
    );
    let (ok, found) = check(&dir);
    assert!(ok, "{found:?}");
    assert!(!codes(&found).contains(&"E024"), "{found:?}");
}

#[test]
fn a_legacy_versioned_entry_still_loads() {
    let dir = greeting_project();
    add_local_greet(&dir);
    // What `add` wrote before D3-b.
    std::fs::write(
        dir.root().join("specforge.json"),
        json!({"name": "p", "version": "0.1.0",
               "extensions": ["@specforge/software", "@sdk/greet@0.1.0"]})
        .to_string(),
    )
    .unwrap();

    let (ok, found) = check(&dir);
    assert!(ok, "{found:?}");
    assert!(!codes(&found).contains(&"E024"), "{found:?}");
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "update never replaces a locally installed extension from a registry"
)]
fn update_never_replaces_a_local_install() {
    // The registry publishes a newer package under the same name.
    let registry = serve(vec![Package::new("@sdk/greet", "9.9.9", greet_wasm())]);
    let dir = project_on(&registry);
    add_local_greet(&dir);
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();

    Cli::on(&dir)
        .command(&["update", "--allow-unsigned"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        registry.requests().len(),
        0,
        "update asked the registry about a local build"
    );
}

#[specforge_test(
    behavior = "batch_update_completed",
    verify = "emits batch_update_completed with correct updatedCount after bulk update"
)]
fn a_bulk_update_reports_batch_update_completed() {
    // greet is locked from the registry one version behind; a local build
    // sits beside it, which a registry never replaces.
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_on(&registry);
    let lock = json!({
        "lockfile_version": 1,
        "entries": [
            {"name": "@sdk/greet", "version": "0.0.9", "source": "registry", "wasm_hash": "00"},
            {"name": "@acme/local", "version": "1.0.0", "source": "local:local.wasm", "wasm_hash": "00"},
        ],
    });
    dir.write("specforge.lock", lock.to_string());

    let out = Cli::on(&dir)
        .command(&["update", "--major", "--allow-unsigned", "--format", "json"])
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let output: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(lock_entry(dir.root(), "@sdk/greet")["version"], "0.1.0");
    let event = &output["batch_update_completed"];
    assert_eq!(event["updatedCount"], 1, "{output}");
    assert_eq!(event["failedCount"], 0, "{output}");
    assert_eq!(event["skippedCount"], 1, "{output}");
    let timestamp = event["timestamp"].as_str().unwrap_or_default();
    assert!(
        chrono::DateTime::parse_from_rfc3339(timestamp).is_ok(),
        "{output}"
    );
}

/// `specforge update --major --allow-unsigned --format json` over a lock
/// with greet one version behind a registry serving 0.1.0, and `second` as
/// the lock's other entry: the exit status and the document, with the clock
/// and the hashes redacted.
fn update_json(second: Value) -> (bool, Value, TestProject) {
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_on(&registry);
    let lock = json!({
        "lockfile_version": 1,
        "entries": [
            {"name": "@sdk/greet", "version": "0.0.9", "source": "registry", "wasm_hash": "00"},
            second,
        ],
    });
    dir.write("specforge.lock", lock.to_string());
    let out = Cli::on(&dir)
        .command(&["update", "--major", "--allow-unsigned", "--format", "json"])
        .output()
        .unwrap();
    let mut document: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("update is not JSON ({e}): {out:?}"));
    document["batch_update_completed"]["timestamp"] = json!("[TIME]");
    let text = redact_hashes(&serde_json::to_string_pretty(&document).unwrap());
    (
        out.status.success(),
        serde_json::from_str(&text).unwrap(),
        dir,
    )
}

/// `text` with every 64-digit hex run replaced by `[SHA256]`.
fn redact_hashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_hexdigit() {
            run.push(c);
            continue;
        }
        out.push_str(if run.len() == 64 { "[SHA256]" } else { &run });
        run.clear();
        out.push(c);
    }
    out.pop();
    out
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "update --format json prints every locked extension's status and the batch_update_completed payload"
)]
fn update_json_lists_every_locked_extension() {
    let local = json!({"name": "@acme/local", "version": "1.0.0", "source": "local:local.wasm", "wasm_hash": "00"});
    let (ok, document, dir) = update_json(local);
    assert!(ok, "{document}");
    assert_eq!(lock_entry(dir.root(), "@sdk/greet")["version"], "0.1.0");
    assert_eq!(document["ok"], true, "{document}");
    let rows = document["extensions"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{document}");
    assert_eq!(rows[0]["status"], "updated", "{document}");
    assert_eq!(rows[0]["name"], "@sdk/greet");
    assert_eq!(rows[0]["publisher"], "unsigned");
    assert!(rows[0].get("key_id").is_none(), "{document}");
    assert_eq!(rows[1]["status"], "not_from_registry", "{document}");
    insta::assert_snapshot!(
        "update_json_lists_every_locked_extension",
        serde_json::to_string_pretty(&document).unwrap()
    );
}

#[specforge_test(
    behavior = "extension_upgrade_completed",
    verify = "emits extension_upgrade_completed with correct previousVersion and newVersion"
)]
fn an_applied_upgrade_reports_extension_upgrade_completed() {
    let local = json!({"name": "@acme/local", "version": "1.0.0", "source": "local:local.wasm", "wasm_hash": "00"});
    let (ok, document, _dir) = update_json(local);
    assert!(ok, "{document}");
    // One payload per extension the update moved; the local build it left
    // alone has none.
    assert_eq!(
        document["extension_upgrade_completed"],
        json!([{
            "extensionName": "@sdk/greet",
            "previousVersion": "0.0.9",
            "newVersion": "0.1.0",
        }]),
        "{document}"
    );

    // An update that failed moved nothing, so nothing completed.
    let gone =
        json!({"name": "@acme/gone", "version": "1.0.0", "source": "registry", "wasm_hash": "00"});
    let (ok, document, _dir) = update_json(gone);
    assert!(!ok, "{document}");
    assert_eq!(
        document["extension_upgrade_completed"],
        json!([]),
        "{document}"
    );
}

#[test]
fn a_failed_update_prints_its_document() {
    let gone =
        json!({"name": "@acme/gone", "version": "1.0.0", "source": "registry", "wasm_hash": "00"});
    let (ok, document, dir) = update_json(gone);
    assert!(!ok, "{document}");
    assert_eq!(lock_entry(dir.root(), "@sdk/greet")["version"], "0.0.9");
    let mut keys: Vec<&str> = document
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "batch_update_completed",
            "extension_upgrade_completed",
            "extensions",
            "ok"
        ],
        "{document}"
    );
    assert_eq!(document["ok"], false, "{document}");
    let rows = document["extensions"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "held_back", "{document}");
    assert_eq!(rows[1]["status"], "failed", "{document}");
    assert_eq!(rows[1]["code"], "R-RES-001", "{document}");
    insta::assert_snapshot!(
        "a_failed_update_prints_its_document",
        serde_json::to_string_pretty(&document).unwrap()
    );
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "update --major on a pre-release lock never installs a lower release"
)]
fn update_major_on_a_pre_release_lock_installs_nothing_lower() {
    // 0.1.0 is a release below the locked 0.2.0-beta.1: --major admits only newer versions.
    let registry = serve(vec![
        Package::new("@sdk/greet", "0.1.0", greet_wasm()),
        Package::new("@sdk/greet", "0.2.0-beta.1", b"\0asm stub".to_vec()),
    ]);
    let dir = project_on(&registry);
    let lock = json!({
        "lockfile_version": 1,
        "entries": [
            {"name": "@sdk/greet", "version": "0.2.0-beta.1", "source": "registry", "wasm_hash": "00"},
        ],
    });
    dir.write("specforge.lock", lock.to_string());
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();

    let out = Cli::on(&dir)
        .command(&["update", "--major", "--allow-unsigned", "--format", "json"])
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let document: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        document["extensions"][0],
        json!({"status": "up_to_date", "name": "@sdk/greet", "version": "0.2.0-beta.1"}),
        "{document}"
    );
    assert_eq!(
        document["extension_upgrade_completed"],
        json!([]),
        "{document}"
    );
    assert_eq!(
        document["batch_update_completed"]["updatedCount"], 0,
        "{document}"
    );
    assert_eq!(
        document["batch_update_completed"]["skippedCount"], 1,
        "{document}"
    );
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
}

/// `specforge doctor --format json` on `root`: whether it passed, and the
/// report.
fn doctor(project: &TestProject) -> (bool, Value) {
    let out = Cli::on(project)
        .env("PATH", crate::toolchain::z3_ready())
        .command(&["doctor", "--format", "json"])
        .output()
        .unwrap();
    let report = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("doctor is not JSON ({e}): {out:?}"));
    (out.status.success(), report)
}

fn finding<'a>(report: &'a Value, code: &str) -> &'a Value {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == code)
        .unwrap_or_else(|| panic!("no {code} finding: {report}"))
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor reports an extension that fails to load (E028, E070) as an error"
)]
fn doctor_reports_an_extension_that_fails_to_load() {
    // A binary that no longer matches its lock entry: check refuses it (E070).
    let tampered = greeting_project();
    add_local_greet(&tampered);
    let installed = tampered
        .root()
        .join(".specforge/extensions/@sdk/greet/extension.wasm");
    let mut bytes = std::fs::read(&installed).unwrap();
    bytes.extend_from_slice(b"tampered");
    std::fs::write(&installed, bytes).unwrap();

    let (ok, report) = doctor(&tampered);
    assert!(!ok, "{report}");
    // One finding, with the command that reinstalls it; the load failure
    // it causes is not listed again.
    let stale = finding(&report, "stale_hash");
    assert_eq!(stale["status"], "error", "{stale}");
    assert!(stale["check"].as_str().unwrap().contains("@sdk/greet"));
    assert!(
        stale["remediation"]
            .as_str()
            .unwrap()
            .starts_with("run `specforge add "),
        "{stale}"
    );
    assert_eq!(stale["about"], "binary", "{report}");
    assert_eq!(stale["issue"]["status"], "stale_hash", "{report}");
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["about"] != "load"),
        "{report}"
    );
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["code"] != "E070"),
        "listed once: {report}"
    );

    // Enabled, but neither locked nor installed: only the load knows (E028).
    let missing = greeting_project();
    add_local_greet(&missing);
    std::fs::remove_file(missing.root().join("specforge.lock")).unwrap();
    std::fs::remove_dir_all(missing.root().join(".specforge")).unwrap();

    let (ok, report) = doctor(&missing);
    assert!(!ok, "{report}");
    assert_eq!(report["ok"], false, "{report}");
    let e028 = finding(&report, "E028");
    assert_eq!(e028["about"], "load", "{report}");
    assert_eq!(e028["status"], "error", "{e028}");
    assert!(
        e028["remediation"]
            .as_str()
            .unwrap()
            .contains("specforge add @sdk/greet"),
        "{e028}"
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "an install that leaves a locked extension's peer unsatisfied is refused before anything is written, local or from a registry"
)]
fn a_local_install_a_locked_extension_does_not_accept_is_refused() {
    let dir = greeting_project();
    dir.write("specforge.json", json!({"name": "p", "version": "0.1.0", "extensions": ["@specforge/software", "@acme/app"]})
            .to_string());
    // `@acme/app` is locked, and wants a newer `@sdk/greet` than the one added.
    let wasm = b"module";
    let installed = dir.root().join(".specforge/extensions/@acme/app");
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::write(installed.join("extension.wasm"), wasm).unwrap();
    let lock = json!({
        "lockfile_version": 1,
        "entries": [
            {"name": "@acme/app", "version": "1.0.0", "source": "registry",
             "wasm_hash": specforge_installed::hex_sha256(wasm),
             "peer_dependencies": [{"name": "@sdk/greet", "version": "^2.0"}]},
        ],
    });
    dir.write("specforge.lock", lock.to_string());
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();
    dir.write("greet.wasm", greet_wasm());

    let out = Cli::on(&dir)
        .command(&["add", "greet.wasm", "--format", "json"])
        .current_dir(dir.root())
        .output()
        .unwrap();

    assert!(!out.status.success());
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("E027"), "{text}");
    assert!(
        text.contains("installing '@sdk/greet' 0.1.0 breaks '@acme/app': "),
        "{text}"
    );
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before,
        "the lock is untouched"
    );
    assert!(!dir.root().join(".specforge/extensions/@sdk/greet").exists());
    assert_eq!(
        enabled(dir.root()),
        json!(["@specforge/software", "@acme/app"])
    );
}

/// Enable `entries` in the project at `root`, replacing what it enabled.
fn enable(project: &TestProject, entries: Value) {
    project.write(
        "specforge.json",
        json!({"name": "p", "version": "0.1.0", "extensions": entries}).to_string(),
    );
}

/// A greeting project with the greet component at `greet.wasm`, not
/// installed: `specforge.json` names the file itself.
fn greet_file_project() -> TestProject {
    greeting_project().file("greet.wasm", greet_wasm())
}

/// Every form a `.wasm` entry takes, for the project at `root`.
fn wasm_entry_forms(root: &Path) -> Vec<String> {
    let absolute = root.join("greet.wasm").display().to_string();
    vec![
        "greet.wasm".to_string(),
        "./greet.wasm".to_string(),
        "@sdk/greet=greet.wasm".to_string(),
        absolute.clone(),
        format!("@sdk/greet={absolute}"),
    ]
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "an entry naming a .wasm file loads that component from disk under the name it declares"
)]
fn a_wasm_file_entry_loads_through_check() {
    let probe = greet_file_project();
    for entry in wasm_entry_forms(probe.root()) {
        let dir = greet_file_project();
        let entry = entry.replace(
            &probe.root().display().to_string(),
            &dir.root().display().to_string(),
        );
        enable(&dir, json!(["@specforge/software", entry]));

        let (ok, found) = check(&dir);

        assert!(ok, "{entry}: {found:?}");
        // `greeting` is a known kind, and greet's field types (W019) and
        // rule (W112) resolved against the extension it declares.
        for code in ["E024", "E028", "E033", "E070", "W019", "W112"] {
            assert!(!codes(&found).contains(&code), "{entry}: {code}: {found:?}");
        }
    }
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "a .wasm file entry is listed under the name it declares, loaded, with source file:<path>"
)]
fn a_wasm_file_entry_is_listed_by_its_declared_name() {
    let dir = greet_file_project();
    enable(&dir, json!(["@specforge/software", "greet.wasm"]));

    let out = Cli::on(&dir)
        .command(&["extensions", "--format", "json"])
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let listed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = listed["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["@sdk/greet", "@specforge/software"], "{listed}");
    let greet = &listed["extensions"][0];
    assert_eq!(greet["status"], "loaded", "{greet}");
    assert_eq!(greet["source"], "file:greet.wasm", "{greet}");
    assert_eq!(greet["version"], "0.1.0", "{greet}");
    assert_eq!(greet["entity_kinds"], json!(["greeting"]), "{greet}");
    assert_eq!(greet["entity_count"], 1, "{greet}");
}

#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "a .wasm file entry that is missing, is not a component, names another extension or repeats a loaded one produces E028 naming the entry"
)]
fn a_wasm_file_entry_that_cannot_load_is_e028_naming_it() {
    let cases = [
        ("missing.wasm", "does not exist"),
        ("bad.wasm", "does not load as an extension component"),
        ("@acme/other=greet.wasm", "declares '@sdk/greet'"),
        ("copy.wasm", "'@sdk/greet' is already loaded"),
    ];
    for (entry, says) in cases {
        let dir = greet_file_project();
        std::fs::write(dir.root().join("bad.wasm"), b"\0asm not a component").unwrap();
        std::fs::write(dir.root().join("copy.wasm"), greet_wasm()).unwrap();
        enable(&dir, json!(["@specforge/software", "greet.wasm", entry]));

        let (ok, found) = check(&dir);

        assert!(!ok, "{entry}: {found:?}");
        let e028: Vec<_> = found.iter().filter(|(code, _, _)| code == "E028").collect();
        assert_eq!(e028.len(), 1, "{entry}: {found:?}");
        let (_, message, suggestion) = e028[0];
        assert!(
            message.contains(&format!("'{entry}'")),
            "{entry}: {message}"
        );
        assert!(message.contains(says), "{entry}: {message}");
        assert!(!suggestion.is_empty(), "{entry}: {message}");
    }
}

#[test]
fn a_legacy_versioned_builtin_entry_still_loads() {
    let dir = greet_file_project();
    // A builtin as `add` wrote it before D3-b.
    enable(&dir, json!(["@specforge/software@0.1.0", "greet.wasm"]));

    let (ok, found) = check(&dir);

    assert!(ok, "{found:?}");
    for code in ["E024", "E028", "I002"] {
        assert!(!codes(&found).contains(&code), "{code}: {found:?}");
    }
}

/// `specforge remove <name> --format json` on `root` (with `extra`
/// arguments): whether it succeeded, and its JSON output.
fn remove(project: &TestProject, name: &str, extra: &[&str]) -> (bool, Value) {
    let mut args = vec!["remove", name, "--format", "json"];
    args.extend(extra);
    let out = Cli::on(project).command(&args).output().unwrap();
    let output = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("remove is not JSON ({e}): {out:?}"));
    (out.status.success(), output)
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "a .wasm file entry is removed by the name it declares or by its entry as written, leaving its file in place"
)]
fn a_wasm_file_entry_is_removed_by_its_declared_name_or_its_entry() {
    let cases = [
        ("greet.wasm", "@sdk/greet"),
        ("greet.wasm", "greet.wasm"),
        ("./greet.wasm", "./greet.wasm"),
        ("@sdk/greet=greet.wasm", "@sdk/greet"),
        ("@sdk/greet=greet.wasm", "@sdk/greet=greet.wasm"),
        ("@sdk/greet=greet.wasm", "greet.wasm"),
    ];
    for (entry, name) in cases {
        let dir = greet_file_project();
        enable(&dir, json!(["@specforge/software", entry]));

        let (ok, output) = remove(&dir, name, &[]);

        assert!(ok, "{entry} by {name}: {output}");
        assert_eq!(
            output["extension"], "@sdk/greet",
            "{entry} by {name}: {output}"
        );
        assert_eq!(
            output["source"],
            format!("file:{}", entry.rsplit('=').next().unwrap())
        );
        assert_eq!(output["version"], "0.1.0", "{output}");
        // `hello` is a greeting, a kind only greet defines.
        assert_eq!(
            output["stranded"],
            json!([{"entity_id": "hello", "kind": "greeting"}]),
            "{entry} by {name}: {output}"
        );
        assert_eq!(enabled(dir.root()), json!(["@specforge/software"]));
        assert!(
            dir.root().join("greet.wasm").is_file(),
            "the file is the user's"
        );
        assert!(!dir.root().join("specforge.lock").exists());
    }

    // An entry whose file does not load is removed by its entry.
    let dir = greet_file_project();
    enable(&dir, json!(["@specforge/software", "missing.wasm"]));
    let (ok, output) = remove(&dir, "missing.wasm", &[]);
    assert!(ok, "{output}");
    assert_eq!(output["extension"], "missing.wasm", "{output}");
    assert_eq!(enabled(dir.root()), json!(["@specforge/software"]));
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "a name more than one specforge.json entry enables is refused as ambiguous, naming the entries"
)]
fn a_name_two_entries_enable_is_refused_as_ambiguous() {
    let cases = [
        // Not installed, so the file is what loads `@sdk/greet`.
        json!(["@sdk/greet", "greet.wasm"]),
        json!(["@sdk/greet=greet.wasm", "@sdk/greet=copy.wasm"]),
    ];
    for entries in cases {
        let dir = greet_file_project();
        std::fs::write(dir.root().join("copy.wasm"), greet_wasm()).unwrap();
        enable(&dir, entries.clone());
        let before = std::fs::read(dir.root().join("specforge.json")).unwrap();

        let (ok, output) = remove(&dir, "@sdk/greet", &[]);

        assert!(!ok, "{entries}: {output}");
        assert_eq!(output["code"], "extension_conflict", "{output}");
        let message = output["error"].as_str().unwrap();
        for entry in entries.as_array().unwrap() {
            assert!(
                message.contains(&format!("'{}'", entry.as_str().unwrap())),
                "{message}"
            );
        }
        assert!(output["suggestion"].as_str().is_some(), "{output}");
        assert_eq!(
            std::fs::read(dir.root().join("specforge.json")).unwrap(),
            before
        );
    }
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "removing a .wasm file entry another extension requires fails with E027 unless --force"
)]
fn removing_a_wasm_file_entry_another_requires_is_e027_unless_forced() {
    let dir = greet_file_project();
    // The dependent is enabled: a locked extension no entry enables is not judged.
    enable(
        &dir,
        json!(["@specforge/software", "greet.wasm", "@acme/uses-greet"]),
    );
    let lock = json!({"lockfile_version": 1,
        "entries": [{"name": "@acme/uses-greet", "version": "1.0.0", "source": "registry",
                     "wasm_hash": "",
                     "peer_dependencies": [{"name": "@sdk/greet", "version": "^0.1.0"}]}],
    });
    std::fs::write(dir.root().join("specforge.lock"), lock.to_string()).unwrap();
    dir.write(".specforge/extensions/@acme/uses-greet/extension.wasm", "");
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();

    let (ok, output) = remove(&dir, "greet.wasm", &[]);
    assert!(!ok, "{output}");
    assert_eq!(output["code"], "E027", "{output}");
    assert!(
        output["error"]
            .as_str()
            .unwrap()
            .contains("@acme/uses-greet"),
        "{output}"
    );
    assert_eq!(
        enabled(dir.root()),
        json!(["@specforge/software", "greet.wasm", "@acme/uses-greet"])
    );

    let (ok, output) = remove(&dir, "@sdk/greet", &["--force"]);
    assert!(ok, "{output}");
    assert_eq!(
        enabled(dir.root()),
        json!(["@specforge/software", "@acme/uses-greet"])
    );
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert!(dir.root().join("greet.wasm").is_file());
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "a locked extension no specforge.json entry enables does not keep its peer from being removed"
)]
fn removing_a_wasm_file_entry_a_disabled_lock_entry_requires_is_accepted() {
    let dir = greet_file_project();
    // `@acme/uses-greet` is locked and requires `@sdk/greet`, but no entry enables it.
    enable(&dir, json!(["@specforge/software", "greet.wasm"]));
    let lock = json!({"lockfile_version": 1,
        "entries": [{"name": "@acme/uses-greet", "version": "1.0.0", "source": "registry",
                     "wasm_hash": "",
                     "peer_dependencies": [{"name": "@sdk/greet", "version": "^0.1.0"}]}],
    });
    std::fs::write(dir.root().join("specforge.lock"), lock.to_string()).unwrap();
    dir.write(".specforge/extensions/@acme/uses-greet/extension.wasm", "");
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();

    let (ok, output) = remove(&dir, "greet.wasm", &[]);

    assert!(ok, "{output}");
    assert_eq!(enabled(dir.root()), json!(["@specforge/software"]));
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert!(dir.root().join("greet.wasm").is_file());
}

#[test]
fn removing_a_wasm_file_no_entry_names_is_not_found() {
    let dir = greet_file_project();
    enable(&dir, json!(["@specforge/software", "greet.wasm"]));

    let (ok, output) = remove(&dir, "other.wasm", &[]);

    assert!(!ok, "{output}");
    assert_eq!(output["code"], "extension_not_found", "{output}");
    assert!(
        output["error"].as_str().unwrap().contains("'other.wasm'"),
        "{output}"
    );
    assert_eq!(
        enabled(dir.root()),
        json!(["@specforge/software", "greet.wasm"])
    );
}

// ── plan 05 pins: the management operations before they take the project view ──

// R1 (plan 05): doctor names a `.wasm` file entry by its file, as the
// extensions listing does (it called it "builtin" before 05-T4).
#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor lists the extensions the extensions listing lists, each with its version, source and status"
)]
fn doctor_lists_a_wasm_file_entry_with_its_file_source() {
    let dir = greet_file_project();
    enable(&dir, json!(["@specforge/software", "greet.wasm"]));

    let (_, report) = doctor(&dir);

    let greet = report["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "@sdk/greet")
        .unwrap_or_else(|| panic!("@sdk/greet not listed: {report}"));
    assert_eq!(greet["source"], "file:greet.wasm", "{report}");

    let out = Cli::on(&dir)
        .command(&["extensions", "--format", "json"])
        .output()
        .unwrap();
    let listed: Value = serde_json::from_slice(&out.stdout).unwrap();
    for extension in report["extensions"].as_array().unwrap() {
        let entry = listed["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == extension["name"])
            .unwrap_or_else(|| panic!("{} not listed: {listed}", extension["name"]));
        assert_eq!(entry["source"], extension["source"], "{extension}");
    }
}

// R2 (plan 05): a removal with an unreadable specforge.json refuses before
// it writes anything (it used to uninstall the binary and empty the lock
// first). Fixed, the same removal finishes cleanly.
#[specforge_test(
    behavior = "remove_extension",
    verify = "a removal with an unreadable specforge.json is config_invalid and changes nothing"
)]
fn a_removal_with_an_unreadable_config_changes_nothing() {
    let dir = greeting_project();
    add_local_greet(&dir);
    let binary = dir
        .root()
        .join(".specforge/extensions/@sdk/greet/extension.wasm");
    assert!(binary.is_file(), "{}", binary.display());
    let broken = r#"{ "extensions": ["@sdk/greet",  }"#;
    std::fs::write(dir.root().join("specforge.json"), broken).unwrap();
    let lock_before = std::fs::read(dir.root().join("specforge.lock")).unwrap();
    let binary_before = std::fs::read(&binary).unwrap();

    let (ok, output) = remove(&dir, "@sdk/greet", &[]);

    assert!(!ok, "{output}");
    assert_eq!(output["code"], "config_invalid", "{output}");
    assert!(
        output["error"]
            .as_str()
            .unwrap()
            .contains("is not valid JSON: expected value at line 1 column"),
        "{output}"
    );
    assert_eq!(
        std::fs::read(dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert_eq!(std::fs::read(&binary).unwrap(), binary_before);
    assert_eq!(
        std::fs::read_to_string(dir.root().join("specforge.json")).unwrap(),
        broken
    );

    // The config fixed, remove finishes: specforge.json, lock and binary.
    enable(&dir, json!(["@specforge/software", "@sdk/greet"]));
    let (ok, output) = remove(&dir, "@sdk/greet", &[]);
    assert!(ok, "{output}");
    assert_eq!(enabled(dir.root()), json!(["@specforge/software"]));
    let lock = std::fs::read_to_string(dir.root().join("specforge.lock")).unwrap();
    assert!(!lock.contains("@sdk/greet"), "{lock}");
    assert!(!dir.root().join(".specforge/extensions/@sdk/greet").exists());
}

/// Every file under `root` with its bytes: what a dry run must leave as it was.
fn tree(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
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

#[specforge_test(
    invariant = "dry_run_side_effect_freedom",
    verify = "--dry-run produces output without modifying files"
)]
fn add_dry_run_prints_the_plan_and_writes_nothing() {
    let dir = greet_file_project();
    enable(&dir, json!([]));
    let before = tree(dir.root());

    // A builtin that requires another: the plan names both, dependencies first.
    let human = Cli::on(&dir)
        .command(&["add", "--dry-run", "@specforge/formal", "greet.wasm"])
        .current_dir(dir.root())
        .output()
        .unwrap();
    assert!(human.status.success(), "{human:?}");
    let text = String::from_utf8(human.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "would enable builtin @specforge/software (required by @specforge/formal)",
            "would enable builtin @specforge/formal",
            "would install @sdk/greet v0.1.0 (local:greet.wasm)",
        ],
        "{text}"
    );

    let json = Cli::on(&dir)
        .command(&["add", "--dry-run", "@specforge/formal", "--format", "json"])
        .output()
        .unwrap();
    assert!(json.status.success(), "{json:?}");
    let document: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(document["added"][0]["action"], "planned", "{document}");
    assert_eq!(
        document["added"][0]["peers_enabled"],
        json!(["@specforge/software"]),
        "{document}"
    );
    assert!(document.get("files_written").is_none(), "{document}");

    assert_eq!(tree(dir.root()), before, "a dry run writes nothing");
}

#[specforge_test(
    invariant = "dry_run_side_effect_freedom",
    verify = "--dry-run produces output without modifying files"
)]
fn remove_dry_run_prints_the_plan_and_writes_nothing() {
    let dir = greet_file_project();
    enable(&dir, json!(["@specforge/software", "greet.wasm"]));
    let before = tree(dir.root());

    let human = Cli::on(&dir)
        .command(&["remove", "--dry-run", "greet.wasm"])
        .output()
        .unwrap();
    assert!(human.status.success(), "{human:?}");
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(
        text.starts_with("Would disable extension '@sdk/greet' (the .wasm file entry greet.wasm;"),
        "{text}"
    );

    let (ok, output) = remove(&dir, "greet.wasm", &["--dry-run"]);
    assert!(ok, "{output}");
    assert_eq!(output["dry_run"], true, "{output}");
    assert!(output.get("files_written").is_none(), "{output}");

    assert_eq!(tree(dir.root()), before, "a dry run writes nothing");
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a dry run answers what the add answers, refusals and the builtins it enables included, and writes nothing"
)]
fn add_dry_run_refuses_what_add_refuses() {
    let dir = greet_file_project();
    enable(&dir, json!(["@x/y"]));
    let lock = json!({"lockfile_version": 1,
        "entries": [{"name": "@x/y", "version": "1.0.0", "source": "registry", "wasm_hash": "",
                     "peer_dependencies": [{"name": "@sdk/greet", "version": "^9.0"}]}],
    });
    std::fs::write(dir.root().join("specforge.lock"), lock.to_string()).unwrap();
    dir.write(".specforge/extensions/@x/y/extension.wasm", "x/y");
    let before = tree(dir.root());

    let run = |dry: bool| {
        let mut command = Cli::on(&dir).command(&["add", "greet.wasm"]);
        if dry {
            command.arg("--dry-run");
        }
        let out = command.output().unwrap();
        (out.status.code(), String::from_utf8(out.stderr).unwrap())
    };
    let (dry_code, dry_stderr) = run(true);
    assert_eq!(tree(dir.root()), before, "a dry run writes nothing");
    let (real_code, real_stderr) = run(false);

    assert_eq!(dry_code, Some(1), "{dry_stderr}");
    assert!(dry_stderr.contains("E027"), "{dry_stderr}");
    assert_eq!((dry_code, dry_stderr), (real_code, real_stderr));
    assert_eq!(tree(dir.root()), before, "a refused add writes nothing");
}

#[test]
fn update_of_a_local_install_says_it_is_not_from_a_registry() {
    let dir = greeting_project();
    add_local_greet(&dir);

    let update = |name: &str| {
        Cli::on(&dir)
            .command(&["update"])
            .arg(name)
            .output()
            .unwrap()
    };
    let out = update("@sdk/greet");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with(
            "@sdk/greet is not from a registry (local:greet.wasm): update never replaces it\n"
        ),
        "{text}"
    );

    // A name the lock does not hold is refused, whatever it is.
    for (name, code) in [
        ("@nope/typo", "extension_not_found"),
        ("Bad Name", "E072"),
        ("@specforge/product", "extension_not_found"),
    ] {
        let out = update(name);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(
            stderr.contains(&format!("error[{code}]")),
            "{name}: {stderr}"
        );
    }
}
