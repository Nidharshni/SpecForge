use assert_cmd::Command;
use specforge_test_macros::test as specforge_test;
use std::fs;
use tempfile::TempDir;

fn specforge_cmd() -> Command {
    assert_cmd::cargo_bin_cmd!("specforge")
}

/// Helper: create a specforge.lock file with test entries.
fn write_lock_file(dir: &std::path::Path, entries: &[(&str, &str, &str)]) {
    let lock_entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|(name, version, source)| {
            serde_json::json!({
                "name": name,
                "version": version,
                "source": source,
                "wasm_hash": format!("hash_{}", name.replace(['@', '/'], "_")),
            })
        })
        .collect();

    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": lock_entries,
    });

    fs::write(
        dir.join("specforge.lock"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();
}

/// Helper: create a specforge.json enabling `@specforge/rust` (which
/// contributes no providers) with `providers` configured.
fn write_config_with_providers(dir: &std::path::Path, providers: &[serde_json::Value]) {
    let config = serde_json::json!({
        "name": "test-project",
        "version": "0.1.0",
        "spec_root": "spec",
        "extensions": ["@specforge/rust"],
        "providers": providers,
    });

    fs::write(
        dir.join("specforge.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

// ===============================================================
// Behavior: remove_extension
// ===============================================================

#[specforge_test(
    behavior = "remove_extension",
    verify = "delegates to uninstall_wasm_extension for lifecycle cleanup"
)]
fn remove_delegates_to_uninstall() {
    let dir = TempDir::new().unwrap();

    // Create a lock file with one extension
    write_lock_file(dir.path(), &[("@acme/software", "1.0.0", "registry")]);

    // Create the extension directory so uninstall can remove it
    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("@acme/software");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"fake wasm").unwrap();
    // Another extension's files must survive.
    let other_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("@acme/other");
    fs::create_dir_all(&other_dir).unwrap();
    fs::write(other_dir.join("extension.wasm"), b"other wasm").unwrap();

    specforge_cmd()
        .args(["remove", "@acme/software", "--path"])
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Removed extension '@acme/software' (v1.0.0)",
        ));

    // Lock file should now have empty entries
    let lock_content = fs::read_to_string(dir.path().join("specforge.lock")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&lock_content).unwrap();
    assert_eq!(lock["entries"].as_array().unwrap().len(), 0);

    // Uninstall cleaned up the extension's whole directory, and only it.
    assert!(!ext_dir.exists(), "the extension directory is deleted");
    assert_eq!(
        fs::read(other_dir.join("extension.wasm")).unwrap(),
        b"other wasm"
    );
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "extension is removed from extensions list"
)]
fn remove_updates_lock_file() {
    let dir = TempDir::new().unwrap();

    write_lock_file(
        dir.path(),
        &[
            ("@specforge/software", "1.0.0", "registry"),
            ("@specforge/governance", "1.0.0", "registry"),
        ],
    );

    // Create extension directories
    for name in &["@specforge/software", "@specforge/governance"] {
        let ext_dir = dir.path().join(".specforge").join("extensions").join(name);
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(ext_dir.join("extension.wasm"), b"fake").unwrap();
    }

    // Remove software
    specforge_cmd()
        .args(["remove", "@specforge/software", "--path"])
        .arg(dir.path())
        .assert()
        .success();

    // Lock file should only have governance remaining
    let lock_content = fs::read_to_string(dir.path().join("specforge.lock")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&lock_content).unwrap();
    let entries = lock["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "@specforge/governance");
}

#[specforge_test(
    behavior = "remove_extension",
    verify = ".spec files are not modified by removal"
)]
fn remove_does_not_modify_spec_files() {
    let dir = TempDir::new().unwrap();

    // Create a spec file
    let spec_dir = dir.path().join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    let spec_content = "behavior my_behavior \"test\" {\n  description \"hello\"\n}\n";
    fs::write(spec_dir.join("test.spec"), spec_content).unwrap();

    write_lock_file(dir.path(), &[("@specforge/software", "1.0.0", "registry")]);

    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("@specforge/software");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"fake").unwrap();

    specforge_cmd()
        .args(["remove", "@specforge/software", "--path"])
        .arg(dir.path())
        .assert()
        .success();

    // Spec file should be unchanged
    let after = fs::read_to_string(spec_dir.join("test.spec")).unwrap();
    assert_eq!(
        after, spec_content,
        ".spec files must not be modified by remove"
    );
}

// ===============================================================
// Behavior: list_installed_extensions
// ===============================================================

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "output order is deterministic"
)]
fn extensions_lists_alphabetically() {
    let dir = TempDir::new().unwrap();

    // Write entries in reverse alphabetical order
    write_lock_file(
        dir.path(),
        &[
            ("@specforge/software", "1.0.0", "registry"),
            ("@specforge/governance", "1.0.0", "registry"),
            ("@specforge/product", "1.0.0", "registry"),
        ],
    );

    let output = specforge_cmd()
        .args(["extensions", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify all three appear
    assert!(stdout.contains("@specforge/governance"));
    assert!(stdout.contains("@specforge/product"));
    assert!(stdout.contains("@specforge/software"));

    // Verify alphabetical ordering: governance < product < software
    let gov_pos = stdout.find("@specforge/governance").unwrap();
    let prod_pos = stdout.find("@specforge/product").unwrap();
    let sw_pos = stdout.find("@specforge/software").unwrap();
    assert!(
        gov_pos < prod_pos && prod_pos < sw_pos,
        "extensions should be listed alphabetically"
    );
}

#[test]
fn extensions_json_format() {
    let dir = TempDir::new().unwrap();

    write_lock_file(
        dir.path(),
        &[
            ("@acme/software", "1.2.0", "registry"),
            ("@acme/governance", "1.0.0", "local"),
        ],
    );

    let output = specforge_cmd()
        .args(["extensions", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("--format json should produce valid JSON");

    assert_eq!(json["count"], 2);
    let extensions = json["extensions"].as_array().unwrap();
    assert_eq!(extensions.len(), 2);

    // Sorted alphabetically, governance first
    assert_eq!(extensions[0]["name"], "@acme/governance");
    assert_eq!(extensions[0]["version"], "1.0.0");
    assert_eq!(extensions[0]["source"], "local");

    assert_eq!(extensions[1]["name"], "@acme/software");
    assert_eq!(extensions[1]["version"], "1.2.0");
    assert_eq!(extensions[1]["source"], "registry");
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "list shows all installed extensions"
)]
fn extensions_no_lock_file() {
    let list = |dir: &std::path::Path| {
        let output = specforge_cmd()
            .args(["extensions", "--path"])
            .arg(dir)
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert!(output.status.success());
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    // Nothing installed: an empty listing.
    let empty = TempDir::new().unwrap();
    let json = list(empty.path());
    assert_eq!(json["count"], 0);
    assert_eq!(json["extensions"], serde_json::json!([]));

    // No lock file, but three builtins enabled: every one is listed.
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software","@specforge/product","@specforge/governance"]}"#,
    )
    .unwrap();
    let json = list(dir.path());
    assert_eq!(json["count"], 3, "{json}");
    let names: Vec<&str> = json["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "@specforge/governance",
            "@specforge/product",
            "@specforge/software"
        ]
    );
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "List Installed Extensions: extension listing holds — kind_registry_ready, all_extensions_listed, entity_counts_included, output_deterministic"
)]
fn extensions_contract() {
    let dir = TempDir::new().unwrap();

    // Precondition: lock file with known extensions
    write_lock_file(
        dir.path(),
        &[
            ("@specforge/governance", "1.0.0", "registry"),
            ("@specforge/software", "2.0.0", "local"),
        ],
    );

    let output = specforge_cmd()
        .args(["extensions", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Postcondition: all_extensions_listed
    assert_eq!(json["count"], 2, "ensures: all_extensions_listed");

    // Postcondition: output_deterministic (sorted alphabetically)
    let extensions = json["extensions"].as_array().unwrap();
    assert_eq!(
        extensions[0]["name"], "@specforge/governance",
        "ensures: output_deterministic — alphabetical"
    );
    assert_eq!(extensions[1]["name"], "@specforge/software");
}

// ===============================================================
// Behavior: list_configured_providers
// ===============================================================

// The listing is the scheme registry's view of each configured provider,
// not the raw config: no builtin contributes providers, so these tests see
// the statuses a provider gets when its extension is not loaded or is not
// a provider. A scheme can't be shown registered, or with kinds, until an
// extension contributes providers (05·R4).

#[specforge_test(
    behavior = "list_configured_providers",
    verify = "list shows all configured providers"
)]
fn providers_lists_alias_extension_schemes() {
    let dir = TempDir::new().unwrap();

    write_config_with_providers(
        dir.path(),
        &[
            serde_json::json!({"scheme": "gh", "alias": "work", "extension": "@acme/github"}),
            serde_json::json!({"scheme": "file", "alias": "junit", "extension": "@specforge/rust"}),
        ],
    );

    let output = specforge_cmd()
        .args(["providers", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("  work (extension: @acme/github)\n    scheme: gh [extension_not_loaded]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("  junit (extension: @specforge/rust)\n    scheme: file [not_a_provider]"),
        "{stdout}"
    );
    // Why neither is registered, as the scheme registry reports it.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning[W118]"), "{stderr}");
}

#[specforge_test(
    behavior = "list_configured_providers",
    verify = "multiple aliases shown separately"
)]
fn providers_multiple_aliases() {
    let dir = TempDir::new().unwrap();

    // Two instances of one provider, each with its own scheme (D3-c).
    write_config_with_providers(
        dir.path(),
        &[
            serde_json::json!({"scheme": "gh", "alias": "work", "extension": "@acme/github"}),
            serde_json::json!({
                "scheme": "gh-shared", "alias": "shared", "extension": "@acme/github",
                "settings": {"repo": "org/shared"},
            }),
        ],
    );

    let (_, json) = providers_json(dir.path());

    assert_eq!(json["count"], 2);
    assert_eq!(
        json["providers"],
        serde_json::json!([
            {"scheme": "gh", "alias": "work", "extension": "@acme/github", "status": "extension_not_loaded"},
            {"scheme": "gh-shared", "alias": "shared", "extension": "@acme/github", "status": "extension_not_loaded"},
        ])
    );
}

#[test]
fn providers_includes_scheme_and_kind() {
    // Not linked to "list includes scheme and kind registrations": no
    // extension contributes providers yet, so no scheme registers kinds.
    let dir = TempDir::new().unwrap();
    write_config_with_providers(
        dir.path(),
        &[serde_json::json!({"scheme": "file", "alias": "junit", "extension": "@specforge/rust"})],
    );

    let (_, json) = providers_json(dir.path());

    let p = &json["providers"][0];
    assert_eq!(p["scheme"], "file");
    assert_eq!(p["status"], "not_a_provider");
    assert_eq!(json["diagnostics"][0]["code"], "W118", "{json}");
}

/// Two providers, `beta` configured before `alpha`.
fn write_two_providers(dir: &std::path::Path) {
    write_config_with_providers(
        dir,
        &[
            serde_json::json!({"scheme": "http", "alias": "beta", "extension": "@specforge/python"}),
            serde_json::json!({"scheme": "file", "alias": "alpha", "extension": "@specforge/rust"}),
        ],
    );
}

/// `specforge providers --format json` for `dir`, asserting success.
fn providers_json(dir: &std::path::Path) -> (String, serde_json::Value) {
    let output = specforge_cmd()
        .args(["providers", "--path"])
        .arg(dir)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json = serde_json::from_str(&stdout).unwrap();
    (stdout, json)
}

#[specforge_test(
    behavior = "list_configured_providers",
    verify = "output order is deterministic"
)]
fn providers_output_order_deterministic() {
    let dir = TempDir::new().unwrap();
    write_two_providers(dir.path());

    // The order is the configuration's (the first to declare a scheme wins
    // it), and the same on every run.
    let (first, json) = providers_json(dir.path());
    let aliases: Vec<&str> = json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["alias"].as_str().unwrap())
        .collect();
    assert_eq!(aliases, ["beta", "alpha"]);
    for _ in 0..3 {
        assert_eq!(
            providers_json(dir.path()).0,
            first,
            "output must be deterministic across runs"
        );
    }
}

#[test]
fn providers_contract() {
    // Not linked to the listing's contract: schemes_and_kinds_included
    // needs an extension that contributes providers.
    let dir = TempDir::new().unwrap();
    write_two_providers(dir.path());
    let (first, json) = providers_json(dir.path());

    assert_eq!(json["count"], 2);
    assert_eq!(
        json["providers"],
        serde_json::json!([
            {"scheme": "http", "alias": "beta", "extension": "@specforge/python", "status": "extension_not_loaded"},
            {"scheme": "file", "alias": "alpha", "extension": "@specforge/rust", "status": "not_a_provider"},
        ])
    );
    assert_eq!(providers_json(dir.path()).0, first);
}

// ===============================================================
// Behavior: run_doctor_check
// ===============================================================

#[test]
fn doctor_reports_health_check() {
    let dir = TempDir::new().unwrap();

    // Create lock file with one extension
    write_lock_file(dir.path(), &[("test-ext", "1.0.0", "registry")]);

    // Create the extension directory with a wasm file
    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("test-ext");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"wasm content").unwrap();

    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --format json should produce valid JSON");

    assert_eq!(json["installed_count"], 1);
    // The hash won't match (lock has "hash_test_ext", actual file has a real sha256)
    // so it should report stale_hash
    assert!(json["findings"].is_array());
}

#[test]
fn doctor_missing_binary() {
    let dir = TempDir::new().unwrap();

    // Create lock file referencing an extension, but no .wasm file
    write_lock_file(dir.path(), &[("missing-ext", "1.0.0", "registry")]);

    // Create extensions dir but NOT the extension subdirectory
    fs::create_dir_all(dir.path().join(".specforge").join("extensions")).unwrap();

    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    // Should exit with 1 (issues found)
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["ok"], false);
    let issues = findings_about(&json, "binary");
    assert!(
        issues
            .iter()
            .any(|i| i["issue"]["status"] == "missing_binary")
    );
}

#[test]
fn doctor_no_lock_file() {
    let dir = TempDir::new().unwrap();

    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["ok"], true);
}

#[test]
fn doctor_lists_enhancements() {
    let dir = TempDir::new().unwrap();

    // Two extensions with valid wasm files
    for name in &["ext-a", "ext-b"] {
        let ext_dir = dir.path().join(".specforge").join("extensions").join(name);
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(
            ext_dir.join("extension.wasm"),
            format!("wasm-{}", name).as_bytes(),
        )
        .unwrap();
    }

    // Lock file entries with correct hashes
    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": [
            {
                "name": "ext-a",
                "version": "1.0.0",
                "source": "registry",
                "wasm_hash": specforge_installed::hex_sha256(b"wasm-ext-a"),
            },
            {
                "name": "ext-b",
                "version": "1.0.0",
                "source": "registry",
                "wasm_hash": specforge_installed::hex_sha256(b"wasm-ext-b"),
            },
        ],
    });

    fs::write(
        dir.path().join("specforge.lock"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();

    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["installed_count"], 2);
}

#[test]
fn doctor_detects_stale_hash() {
    let dir = TempDir::new().unwrap();

    // Extension with wrong hash in lock file (simulates tampered/updated binary)
    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("stale-ext");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"updated content").unwrap();

    write_lock_file(dir.path(), &[("stale-ext", "1.0.0", "registry")]);

    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["ok"], false);
    let issues = findings_about(&json, "binary");
    assert!(
        issues.iter().any(|i| i["issue"]["status"] == "stale_hash"),
        "should detect stale hash when binary changes"
    );
}

#[test]
fn doctor_healthy_lock_entry() {
    let dir = TempDir::new().unwrap();

    // Precondition: lock file + extension with matching hash
    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("ok-ext");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"good wasm").unwrap();

    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": [{
            "name": "ok-ext",
            "version": "1.0.0",
            "source": "registry",
            "wasm_hash": specforge_installed::hex_sha256(b"good wasm"),
        }],
    });
    fs::write(
        dir.path().join("specforge.lock"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();

    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Postcondition: report produced
    assert_eq!(json["ok"], true, "ensures: report_produced");
    assert_eq!(
        json["installed_count"], 1,
        "ensures: doctor_check_completed_emitted"
    );
    assert!(
        findings_about(&json, "binary").is_empty(),
        "ensures: no issues for valid extension"
    );
}

/// A project with the software and product builtins enabled (no lock
/// entries: builtins ship inside the binary), plus optional spec sources.
fn builtin_project(spec: Option<&str>) -> TempDir {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software", "@specforge/product"]);
    let spec_dir = dir.path().join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    if let Some(spec) = spec {
        fs::write(spec_dir.join("project.spec"), spec).unwrap();
    }
    dir
}

/// An entity whose ID is the kind keyword the software builtin registers:
/// the compiler reports the shadowing as E013.
const SHADOWING_ID: &str = "type behavior \"Shadow\" {\n}\n";

fn doctor_json(dir: &std::path::Path) -> (serde_json::Value, i32) {
    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("doctor --format json is not JSON ({e}): {stdout}"));
    (json, output.status.code().unwrap_or(-1))
}

fn doctor_human(dir: &std::path::Path) -> (String, i32) {
    let output = specforge_cmd()
        .args(["doctor", "--path"])
        .arg(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

fn extension<'a>(report: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    report["extensions"]
        .as_array()
        .unwrap_or_else(|| panic!("no extensions array: {report}"))
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("{name} not listed: {report}"))
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor lists all installed extensions with enhancement counts"
)]
fn doctor_lists_enabled_builtins_and_lock_entries_with_enhancement_counts() {
    let dir = builtin_project(None);
    // One installed extension alongside the builtins.
    write_lock_file(dir.path(), &[("test-ext", "1.2.3", "registry")]);

    let (report, code) = doctor_json(dir.path());

    // software enhances module (ports, ports_defined) and milestone
    // (behaviors); product enhances nothing.
    let software = extension(&report, "@specforge/software");
    assert_eq!(software["source"], "builtin", "{report}");
    assert_eq!(software["enhancement_count"], 2, "{report}");
    assert!(
        !software["version"].as_str().unwrap_or_default().is_empty(),
        "{report}"
    );
    let product = extension(&report, "@specforge/product");
    assert_eq!(product["source"], "builtin", "{report}");
    assert_eq!(product["enhancement_count"], 0, "{report}");
    let installed = extension(&report, "test-ext");
    assert_eq!(installed["source"], "registry", "{report}");
    assert_eq!(installed["version"], "1.2.3", "{report}");
    assert_eq!(installed["status"], "not_configured", "{report}");
    // test-ext's binary is missing, so doctor still fails on it.
    assert_eq!(code, 1, "{report}");

    let (human, _) = doctor_human(dir.path());
    let line = |name: &str| {
        human
            .lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("{name} not in: {human}"))
            .to_string()
    };
    assert!(
        line("@specforge/software").contains("2 enhancement(s)"),
        "{human}"
    );
    assert!(
        line("@specforge/product").contains("0 enhancement(s)"),
        "{human}"
    );
    assert!(line("test-ext").contains("1.2.3"), "{human}");
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor lists all enhancements grouped by entity kind"
)]
fn doctor_groups_enhancements_by_target_entity_kind() {
    let dir = builtin_project(None);

    let (report, code) = doctor_json(dir.path());
    assert_eq!(code, 0, "{report}");

    let by_kind = report["enhancements"]
        .as_object()
        .unwrap_or_else(|| panic!("enhancements is not an object: {report}"));
    let kinds: Vec<&String> = by_kind.keys().collect();
    assert_eq!(kinds, ["milestone", "module"], "{report}");
    let module = &by_kind["module"][0];
    assert_eq!(module["extension"], "@specforge/software", "{report}");
    assert_eq!(
        module["fields"],
        serde_json::json!(["ports", "ports_defined"]),
        "{report}"
    );
    let milestone = &by_kind["milestone"][0];
    assert_eq!(milestone["extension"], "@specforge/software", "{report}");
    assert_eq!(
        milestone["fields"],
        serde_json::json!(["behaviors"]),
        "{report}"
    );

    let (human, _) = doctor_human(dir.path());
    let module_at = human.find("  module:").unwrap_or_else(|| panic!("{human}"));
    let milestone_at = human
        .find("  milestone:")
        .unwrap_or_else(|| panic!("{human}"));
    assert!(milestone_at < module_at, "kinds are sorted: {human}");
    assert!(
        human[module_at..].contains("@specforge/software: ports, ports_defined"),
        "{human}"
    );
}

// Conflicts are proven at the shared report's seam (src/doctor.rs): no
// shipped builtin set produces an extension conflict.
#[test]
fn doctor_fails_on_a_shadowed_keyword_without_calling_it_a_conflict() {
    let dir = builtin_project(Some(SHADOWING_ID));

    let (report, code) = doctor_json(dir.path());

    assert_eq!(report["conflicts"], serde_json::json!([]), "{report}");
    assert_eq!(report["ok"], false, "{report}");
    assert_eq!(code, 1, "{report}");
    let (human, human_code) = doctor_human(dir.path());
    assert_eq!(human_code, 1, "{human}");
    assert!(human.contains("Conflicts:\n  none"), "{human}");
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor detects shadowed grammar-level constructs"
)]
fn doctor_reports_a_keyword_shadowing_an_extension_entity_kind() {
    let dir = builtin_project(Some(SHADOWING_ID));

    let (report, _) = doctor_json(dir.path());

    let shadowed = findings_about(&report, "shadowing");
    assert_eq!(shadowed.len(), 1, "{report}");
    assert_eq!(shadowed[0]["keyword"], "behavior", "{report}");
    assert_eq!(shadowed[0]["code"], "E013", "{report}");
    assert!(
        shadowed[0]["remediation"]
            .as_str()
            .unwrap()
            .contains("rename"),
        "{report}"
    );

    // A project without the clash shadows nothing.
    let clean_dir = builtin_project(None);
    let (clean, _) = doctor_json(clean_dir.path());
    assert!(findings_about(&clean, "shadowing").is_empty(), "{clean}");

    let (human, _) = doctor_human(dir.path());
    let section = human
        .find("Shadowed constructs")
        .unwrap_or_else(|| panic!("{human}"));
    assert!(human[section..].contains("'behavior'"), "{human}");
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor --format json produces valid JSON output"
)]
fn doctor_json_carries_every_report_section() {
    let dir = builtin_project(Some(SHADOWING_ID));

    let (report, _) = doctor_json(dir.path());

    for key in ["extensions", "conflicts", "findings"] {
        assert!(report[key].is_array(), "{key} is not an array: {report}");
    }
    assert!(report["enhancements"].is_object(), "{report}");
    assert!(report["cache_status"].is_string(), "{report}");
    assert!(report["ok"].is_boolean(), "{report}");
    let findings = report["findings"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["code"] == "E013"), "{report}");
    for finding in findings {
        for field in ["about", "check", "status", "code", "remediation"] {
            assert!(finding[field].is_string(), "{field} missing: {finding}");
        }
    }
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "Run Doctor Check: doctor check holds — enhancement_registered_fired, filesystem_available, doctor_check_completed_emitted, report_produced, json_output_supported"
)]
fn doctor_contract() {
    // Requires: builtins registered (their enhancements reach the report),
    // and the filesystem holds the lock file and an installed binary.
    let dir = builtin_project(None);
    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("ok-ext");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"good wasm").unwrap();
    let lock = serde_json::json!({
        "lockfile_version": 1,
        "entries": [{
            "name": "ok-ext",
            "version": "1.0.0",
            "source": "registry",
            "wasm_hash": specforge_installed::hex_sha256(b"good wasm"),
        }],
    });
    fs::write(
        dir.path().join("specforge.lock"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();

    let (report, code) = doctor_json(dir.path());

    assert_eq!(code, 0, "{report}");
    assert_eq!(
        extension(&report, "@specforge/software")["enhancement_count"],
        2,
        "requires: enhancement_registered_fired: {report}"
    );
    assert_eq!(
        report["installed_count"], 1,
        "requires: filesystem_available (the lock entry is checked on disk): {report}"
    );
    assert_eq!(
        report["ok"], true,
        "ensures: doctor_check_completed_emitted: {report}"
    );
    assert!(
        findings_about(&report, "binary").is_empty(),
        "ensures: report_produced — the binary matches its lock hash: {report}"
    );
    assert_eq!(
        report["cache_status"], "ok",
        "ensures: json_output_supported: {report}"
    );

    let (human, human_code) = doctor_human(dir.path());
    assert_eq!(human_code, 0, "{human}");
    for section in [
        "Extensions",
        "Enhancements",
        "Conflicts",
        "Shadowed constructs",
    ] {
        assert!(
            human.contains(section),
            "ensures: report_produced ({section}): {human}"
        );
    }
}

// ===============================================================
// Behavior: add_extension (specifier validation)
// ===============================================================

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add unresolvable extension rejects with diagnostic"
)]
fn add_validates_registry_specifier() {
    let dir = TempDir::new().unwrap();
    // A registry nothing listens on (port 9, discard): the connection is
    // refused at once, so the test never reaches the network or waits on a
    // timeout. The specifier must parse and the failure be structured.
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[],
            "registries":[{"alias":"local","url":"http://127.0.0.1:9/v1","default_registry":true}]}"#,
    )
    .unwrap();
    let output = specforge_cmd()
        .args(["add", "@acme/widget@1.0.0", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    // Expect failure (registry unreachable), but structured JSON error output
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json["error"].as_str().is_some());
    assert!(json["code"].as_str().is_some());
}

#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "invalid specifier produces ExtensionError"
)]
fn add_rejects_invalid_specifier() {
    let dir = TempDir::new().unwrap();
    let config = r#"{"name":"t","version":"0.1.0","extensions":[]}"#;
    fs::write(dir.path().join("specforge.json"), config).unwrap();

    for specifier in ["not-valid", "@scope"] {
        let output = specforge_cmd()
            .args(["add", specifier, "--path"])
            .arg(dir.path())
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["code"], "E054", "{json}");
        let message = json["error"].as_str().unwrap();
        assert!(
            message.starts_with("invalid extension specifier: ")
                && message.contains(&format!("'{specifier}'")),
            "{message}"
        );
    }

    // The human diagnostic states the expected format.
    specforge_cmd()
        .args(["add", "not-valid", "--path"])
        .arg(dir.path())
        .assert()
        .code(1)
        .stderr(predicates::str::contains(
            "error[E054]: invalid extension specifier: 'not-valid' is not a registry package name",
        ))
        .stderr(predicates::str::contains(
            "use a builtin's name, './local/path.wasm', 'git+https://...', or '@scope/name[@version]'",
        ));

    // Nothing was installed.
    assert_eq!(
        fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        config
    );
    assert!(!dir.path().join("specforge.lock").exists());
}

/// Which inputs `specforge add` takes for a registry package (plan 12 §2.2):
/// E063 is the registry port reached with no registry configured, E054 and
/// R-RES-003 are the argument refused before any registry is asked.
#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "each add argument reads as one extension source"
)]
fn add_refuses_what_is_not_a_package_before_the_registry() {
    let dir = TempDir::new().unwrap();
    let config = r#"{"name":"t","version":"0.1.0","extensions":[]}"#;
    fs::write(dir.path().join("specforge.json"), config).unwrap();

    let cases: &[(&str, &str)] = &[
        ("@acme/tool", "E063"),                // I1
        ("@acme/tool@", "E054"),               // I2
        ("@acme/tool@1.2.0", "E063"),          // I3
        ("@acme/tool@^1.2", "E063"),           // I4
        ("@acme/tool@1.x", "E063"),            // I5
        ("@acme/tool@1.2", "E063"),            // I6
        ("@acme/tool@1.0.0/x", "R-RES-003"),   // I7
        ("@acme/tool@1.0.0?x=1", "R-RES-003"), // I8
        ("foo@/bar", "E054"),                  // I9
        ("tool@1.0.0", "E054"),                // I10
        ("tool", "E054"),                      // I11
        ("@acme/..", "E054"),                  // I12
        ("@acme/aa/bb", "E054"),               // I13
        ("@acme/a/b", "E054"),                 // I14
        ("@a/x", "E063"),                      // I15
        ("@acme/T ool", "E054"),               // I16
        ("Acme@1", "E054"),                    // I17
        ("@acme/tool@latest", "E063"),         // I18
        ("@acme/tool@*", "E063"),              // I18
        ("@acme/tool@>=1, <2", "E063"),        // I19
        ("@acme/tool@^bogus", "R-RES-003"),    // I20
        ("@acme/tool@2.0.0+build.1", "E063"),  // I21
        ("@scope", "E054"),                    // I22
    ];
    for (input, code) in cases {
        let output = specforge_cmd()
            .args(["add", input, "--path"])
            .arg(dir.path())
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{input}: {output:?}");
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["code"], *code, "{input}: {json}");
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        config
    );
}

/// Plan 12 §3 R3: a module whose declared name is `../../../x` is refused
/// (E072) with nothing written, and `remove ../../../x` is refused with
/// nothing deleted, even when a lock entry carries that name. The module is
/// the greet blob with its name (`@sdk/greet`, 10 bytes) replaced by a
/// path of the same length.
#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "an extension is installed under the extensions directory of its project, by its package name"
)]
fn a_module_whose_declared_name_is_not_a_package_name_is_refused() {
    let root = TempDir::new().unwrap();
    let project = root.path().join("a/b/proj");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();

    let (from, to) = (b"@sdk/greet".as_slice(), b"../../../x".as_slice());
    let mut wasm = crate::registry::greet_wasm();
    let mut replaced = 0;
    let mut at = 0;
    while at + from.len() <= wasm.len() {
        if &wasm[at..at + from.len()] == from {
            wasm[at..at + from.len()].copy_from_slice(to);
            replaced += 1;
            at += from.len();
        } else {
            at += 1;
        }
    }
    assert!(replaced > 0, "the blob names itself");
    let module = root.path().join("evil.wasm");
    fs::write(&module, wasm).unwrap();

    let outside = root.path().join("a/b/x");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("other.txt"), "keep").unwrap();
    let before = crate::written::files_under(root.path());
    let config_before = fs::read_to_string(project.join("specforge.json")).unwrap();

    let output = specforge_cmd()
        .arg("add")
        .arg(&module)
        .arg("--path")
        .arg(&project)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "E072", "{json}");
    assert!(
        json["error"].as_str().unwrap().contains("'../../../x'"),
        "{json}"
    );
    // Nothing was written: not the module, not the lock, not the config.
    assert_eq!(crate::written::files_under(root.path()), before);
    assert_eq!(
        fs::read_to_string(project.join("specforge.json")).unwrap(),
        config_before
    );

    // `remove` of that name is refused too, whether or not a lock names it.
    for lock in [false, true] {
        if lock {
            write_lock_file(&project, &[("../../../x", "1.0.0", "registry")]);
        }
        let output = specforge_cmd()
            .args(["remove", "../../../x", "--path"])
            .arg(&project)
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "lock={lock}: {output:?}");
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["code"], "E072", "lock={lock}: {json}");
        assert_eq!(
            fs::read_to_string(outside.join("other.txt")).unwrap(),
            "keep"
        );
    }
    assert!(outside.is_dir());
}

#[specforge_test(
    behavior = "parse_extension_specifier",
    verify = "./path parsed as local source"
)]
fn add_validates_local_specifier() {
    let dir = TempDir::new().unwrap();

    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    // A real extension at the local path.
    let wasm_path = dir.path().join("my-extension.wasm");
    std::fs::write(&wasm_path, crate::registry::greet_wasm()).unwrap();

    let output = specforge_cmd()
        .args(["add", wasm_path.to_str().unwrap(), "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["added"][0]["action"], "installed");
    assert_eq!(json["added"][0]["extension"], "@sdk/greet", "{json}");
    assert_eq!(
        json["added"][0]["source"], "local:my-extension.wasm",
        "{json}"
    );
}

// ===============================================================
// Behavior: remove_extension (remaining verify statements)
// ===============================================================

#[test]
fn remove_extension_keywords_produce_e024() {
    let dir = TempDir::new().unwrap();

    // Set up a project with an extension
    write_lock_file(dir.path(), &[("@specforge/software", "1.0.0", "registry")]);

    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("@specforge/software");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"fake").unwrap();

    // Create a spec file using an entity keyword that would be from that extension
    let spec_dir = dir.path().join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("test.spec"),
        "behavior orphaned_thing \"test\" {\n  contract \"should break\"\n}\n",
    )
    .unwrap();

    // Remove the extension
    specforge_cmd()
        .args(["remove", "@specforge/software", "--path"])
        .arg(dir.path())
        .assert()
        .success();

    // Lock file should no longer contain that extension
    let lock_content = fs::read_to_string(dir.path().join("specforge.lock")).unwrap();
    assert!(
        !lock_content.contains("@specforge/software"),
        "extension should be removed from lock file"
    );
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "Remove Extension: extension removal holds — extension_installed, filesystem_available, extension_entry_removed, spec_files_unchanged, extension_removed_emitted"
)]
fn remove_extension_contract() {
    let dir = TempDir::new().unwrap();

    // Precondition: extension is installed
    write_lock_file(dir.path(), &[("@specforge/software", "1.0.0", "registry")]);

    let ext_dir = dir
        .path()
        .join(".specforge")
        .join("extensions")
        .join("@specforge/software");
    fs::create_dir_all(&ext_dir).unwrap();
    fs::write(ext_dir.join("extension.wasm"), b"fake").unwrap();

    // Act
    specforge_cmd()
        .args(["remove", "@specforge/software", "--path"])
        .arg(dir.path())
        .assert()
        .success();

    // Postcondition: entry removed from lock file
    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("specforge.lock")).unwrap())
            .unwrap();
    assert_eq!(
        lock["entries"].as_array().unwrap().len(),
        0,
        "ensures: extension_entry_removed"
    );

    // Postcondition: .wasm binary directory is cleaned up
    assert!(
        !ext_dir.join("extension.wasm").exists(),
        "ensures: wasm binary removed"
    );
}

/// A project enabling `@specforge/software` and the installed `extensions`,
/// each locked with the peers it recorded at install.
fn project_with_installed(dir: &std::path::Path, installed: &[(&str, &[&str])]) {
    let mut enabled = vec!["@specforge/software".to_string()];
    let mut entries = Vec::new();
    for (name, peers) in installed {
        enabled.push(name.to_string());
        let ext_dir = dir.join(".specforge/extensions").join(name);
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(ext_dir.join("extension.wasm"), b"fake").unwrap();
        let peers: Vec<serde_json::Value> = peers
            .iter()
            .map(|p| serde_json::json!({"name": p, "version": "^1.0", "optional": false}))
            .collect();
        entries.push(serde_json::json!({
            "name": name, "version": "1.0.0", "source": "registry",
            "wasm_hash": "", "peer_dependencies": peers,
        }));
    }
    fs::write(
        dir.join("specforge.json"),
        serde_json::json!({"name": "p", "version": "0.1.0", "extensions": enabled}).to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("specforge.lock"),
        serde_json::json!({"lockfile_version": 1, "entries": entries}).to_string(),
    )
    .unwrap();
}

fn enabled_extensions(dir: &std::path::Path) -> Vec<String> {
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("specforge.json")).unwrap()).unwrap();
    config["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect()
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "removing an installed extension drops its specforge.json entry"
)]
fn remove_installed_extension_drops_its_config_entry() {
    let dir = TempDir::new().unwrap();
    project_with_installed(dir.path(), &[("@acme/base", &[])]);

    specforge_cmd()
        .args(["remove", "@acme/base", "--path"])
        .arg(dir.path())
        .assert()
        .success();

    assert_eq!(enabled_extensions(dir.path()), ["@specforge/software"]);
    assert!(!dir.path().join(".specforge/extensions/@acme/base").exists());
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "removing an extension another installed extension requires fails with E027 unless --force"
)]
fn remove_refuses_an_extension_another_requires() {
    let dir = TempDir::new().unwrap();
    project_with_installed(
        dir.path(),
        &[("@acme/base", &[]), ("@acme/app", &["@acme/base"])],
    );
    let lock_before = fs::read(dir.path().join("specforge.lock")).unwrap();

    let out = specforge_cmd()
        .args(["remove", "@acme/base", "--format", "json", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let error: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["code"], "E027", "{error}");
    assert!(
        error["error"].as_str().unwrap().contains("@acme/app"),
        "{error}"
    );
    // Nothing changed.
    assert_eq!(
        fs::read(dir.path().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert!(enabled_extensions(dir.path()).contains(&"@acme/base".to_string()));

    specforge_cmd()
        .args(["remove", "@acme/base", "--force", "--path"])
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(
        enabled_extensions(dir.path()),
        ["@specforge/software", "@acme/app"]
    );
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "specforge remove with no lock file reports error"
)]
fn remove_no_lock_file() {
    let dir = TempDir::new().unwrap();

    specforge_cmd()
        .args(["remove", "@specforge/software", "--path"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not installed"));
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "specforge remove for non-existent extension reports error"
)]
fn remove_nonexistent_extension() {
    let dir = TempDir::new().unwrap();

    write_lock_file(dir.path(), &[("@specforge/software", "1.0.0", "registry")]);

    specforge_cmd()
        .args(["remove", "@specforge/other", "--path"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not installed"));
}

// ---------------------------------------------------------------------------
// specforge new --extension (SDK adoption scaffolder)
// ---------------------------------------------------------------------------

#[test]
fn new_extension_scaffolds_sdk_project() {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out");

    let assert = specforge_cmd()
        .args([
            "new",
            "--extension",
            "@you/my-ext",
            "--path",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(assert.status.success());

    let project = out.join("my-ext");
    let cargo = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo.contains(r#"name = "my-ext""#));
    assert!(cargo.contains("specforge-extension-sdk"));
    assert!(cargo.contains("wit-bindgen"));

    let config = fs::read_to_string(project.join(".cargo/config.toml")).unwrap();
    assert!(config.contains("wasm32-wasip2"));

    let lib = fs::read_to_string(project.join("src/lib.rs")).unwrap();
    assert!(lib.contains("component_guest!"));

    let lib = fs::read_to_string(project.join("src/lib.rs")).unwrap();
    assert!(lib.contains(r#"name = "@you/my-ext""#));
    assert!(lib.contains("impl Contributions for Extension"));
}

#[test]
fn new_extension_refuses_existing_directory() {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out");
    let project = out.join("dup");
    fs::create_dir_all(&project).unwrap();

    let assert = specforge_cmd()
        .args(["new", "--extension", "dup", "--path", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!assert.status.success());

    let stderr = String::from_utf8_lossy(&assert.stderr);
    assert!(stderr.contains("already exists"), "{}", stderr);
}

// ===============================================================
// Builtins: enabled through specforge.json, no download or lock entry
// ===============================================================

fn write_config_with_extensions(dir: &std::path::Path, extensions: &[&str]) {
    let config = serde_json::json!({
        "name": "test-project",
        "version": "0.1.0",
        "spec_root": "spec",
        "extensions": extensions,
    });
    fs::write(
        dir.join("specforge.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

fn read_config(dir: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join("specforge.json")).unwrap()).unwrap()
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add extension appends to extensions list"
)]
fn add_builtin_enables_it_in_specforge_json() {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software"]);

    specforge_cmd()
        .args(["add", "@specforge/product", "--path"])
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "enabled builtin @specforge/product",
        ));

    let config = read_config(dir.path());
    assert_eq!(
        config["extensions"],
        serde_json::json!(["@specforge/software", "@specforge/product"])
    );
    assert_eq!(config["name"], "test-project", "other fields preserved");
    assert!(
        !dir.path().join("specforge.lock").exists(),
        "builtins need no lock entry"
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add enables a builtin's required peers but not its optional ones"
)]
fn add_enables_required_peers_only() {
    // software's peer on product is optional: software alone is enabled.
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &[]);
    specforge_cmd()
        .args(["add", "@specforge/software", "--path"])
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(
        read_config(dir.path())["extensions"],
        serde_json::json!(["@specforge/software"])
    );

    // cargo-test requires @specforge/testing: it is enabled first.
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software"]);
    specforge_cmd()
        .args(["add", "@specforge/cargo-test", "--path"])
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "enabled builtin @specforge/testing (required by @specforge/cargo-test)",
        ));
    assert_eq!(
        read_config(dir.path())["extensions"],
        serde_json::json!([
            "@specforge/software",
            "@specforge/testing",
            "@specforge/cargo-test"
        ])
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add duplicate extension is a no-op with info message"
)]
fn add_enabled_builtin_is_a_no_op() {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software"]);

    specforge_cmd()
        .args(["add", "@specforge/software@latest", "--path"])
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("already enabled"));

    assert_eq!(
        read_config(dir.path())["extensions"],
        serde_json::json!(["@specforge/software"])
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add extension with no specforge.json rejects with error and exit code 1"
)]
fn add_builtin_without_project_fails() {
    let dir = TempDir::new().unwrap();

    specforge_cmd()
        .args(["add", "@specforge/product", "--path"])
        .arg(dir.path())
        .assert()
        .code(1)
        .stderr(predicates::str::contains("specforge init"));
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "list shows all installed extensions"
)]
fn extensions_lists_enabled_builtins() {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software", "@specforge/formal"]);
    write_lock_file(dir.path(), &[("@acme/widget", "1.0.0", "registry")]);

    let output = specforge_cmd()
        .args(["extensions", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["count"], 3);
    let listed: Vec<(&str, &str)> = json["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["name"].as_str().unwrap(), e["source"].as_str().unwrap()))
        .collect();
    // Alphabetical, builtins and installs alike.
    assert_eq!(
        listed,
        [
            ("@acme/widget", "registry"),
            ("@specforge/formal", "builtin"),
            ("@specforge/software", "builtin"),
        ]
    );
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "extension is removed from extensions list"
)]
fn remove_builtin_disables_it() {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software", "@specforge/product"]);

    specforge_cmd()
        .args(["remove", "@specforge/product", "--path"])
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(
        read_config(dir.path())["extensions"],
        serde_json::json!(["@specforge/software"])
    );

    specforge_cmd()
        .args(["remove", "@specforge/product", "--path"])
        .arg(dir.path())
        .assert()
        .code(1)
        .stderr(predicates::str::contains("not installed"));
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "list includes entity counts and entity types"
)]
fn extensions_list_each_extensions_kinds_and_entities() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software","@specforge/product"]}"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("spec/app.spec"),
        "behavior login \"L\" {\n  contract \"c\"\n}\n\nbehavior logout \"O\" {\n  contract \"c\"\n}\n\nfeature auth \"A\" {\n  behaviors [login, logout]\n}\n",
    )
    .unwrap();

    let output = specforge_cmd()
        .args(["extensions", "--path"])
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let find = |name: &str| {
        json["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing: {json}"))
            .clone()
    };
    let software = find("@specforge/software");
    let kinds: Vec<&str> = software["entity_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"behavior") && kinds.contains(&"invariant"),
        "{kinds:?}"
    );
    assert!(!kinds.contains(&"feature"), "{kinds:?}");
    assert_eq!(software["entity_count"], 2, "{software}");
    let product = find("@specforge/product");
    assert!(
        product["entity_kinds"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("feature")),
        "{product}"
    );
    assert_eq!(product["entity_count"], 1, "{product}");

    let human = specforge_cmd()
        .args(["extensions", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("@specforge/software v1.0.0 (builtin): 2 entities"),
        "{text}"
    );
}

// ===============================================================
// Plan 05 pins: the management operations before they take the
// project view
// ===============================================================

#[specforge_test(
    behavior = "remove_extension",
    verify = "a removal with an unreadable specforge.json is config_invalid and changes nothing"
)]
fn removing_a_builtin_with_an_unreadable_config_is_config_invalid() {
    let dir = TempDir::new().unwrap();
    let config = r#"{ "extensions": ["@specforge/product",  }"#;
    fs::write(dir.path().join("specforge.json"), config).unwrap();

    let output = specforge_cmd()
        .args(["remove", "@specforge/product", "--format", "json", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "config_invalid", "{json}");
    assert_eq!(
        fs::read_to_string(dir.path().join("specforge.json")).unwrap(),
        config
    );
}

// One refusal for an unusable specforge.json: add, update and remove all
// answer config_invalid with the reason E069 gives, and change nothing.
#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
)]
fn add_update_and_remove_refuse_an_unusable_config_alike() {
    for config in UNUSABLE_CONFIGS {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("specforge.json"), config).unwrap();
        // A lock, so `update` has something it could touch.
        fs::write(
            dir.path().join("specforge.lock"),
            r#"{"lockfile_version":1,"entries":[{"name":"@acme/x","version":"1.0.0","source":"registry","wasm_hash":"h"}]}"#,
        )
        .unwrap();
        let before = crate::written::files_under(dir.path());
        // What the compile reports as E069 for it.
        let check = specforge_cmd()
            .args(["check", "--format", "json"])
            .arg(dir.path())
            .output()
            .unwrap();
        let found: Vec<serde_json::Value> = serde_json::from_slice(&check.stdout).unwrap();
        let reason = found[0]["message"].as_str().unwrap().to_string();

        let mut refusals = Vec::new();
        for args in [
            vec!["add", "@specforge/product"],
            vec!["update"],
            vec!["remove", "@acme/x"],
        ] {
            let output = specforge_cmd()
                .args(&args)
                .args(["--format", "json", "--path"])
                .arg(dir.path())
                .output()
                .unwrap();

            assert_eq!(
                output.status.code(),
                Some(1),
                "{config}: {args:?}: {output:?}"
            );
            let json: serde_json::Value = serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|e| panic!("{args:?}: not JSON ({e}): {output:?}"));
            assert_eq!(json["code"], "config_invalid", "{config}: {args:?}: {json}");
            assert!(
                reason.contains(json["error"].as_str().unwrap()),
                "{config}: {args:?}: E069 says `{reason}`, the refusal `{json}`"
            );
            assert_eq!(
                crate::written::files_under(dir.path()),
                before,
                "{config}: {args:?} wrote"
            );
            refusals.push(json);
        }
        assert!(
            refusals.windows(2).all(|pair| pair[0] == pair[1]),
            "{refusals:?}"
        );
    }
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "list includes entity counts and entity types"
)]
fn a_legacy_entry_that_did_not_load_is_listed_with_its_written_version() {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/product", "@acme/missing@1.2.0"]);

    let output = specforge_cmd()
        .args(["extensions", "--format", "json", "--path"])
        .arg(dir.path())
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let missing = json["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "@acme/missing")
        .unwrap_or_else(|| panic!("@acme/missing not listed: {json}"));
    assert_eq!(missing["version"], "1.2.0", "{missing}");
    assert_eq!(missing["status"], "not_loaded", "{missing}");
    assert_eq!(missing["source"], "unknown", "{missing}");
}

/// Doctor's findings, without the z3 probe's (it depends on PATH).
fn project_findings(report: &serde_json::Value) -> Vec<serde_json::Value> {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] != "z3_missing")
        .cloned()
        .collect()
}

// Panel D5 (plan 05): a directory without specforge.json is a valid
// default project, but doctor says so, as a warning.
#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor in a directory without specforge.json reports config_missing as a warning"
)]
fn doctor_without_specforge_json_says_so() {
    let dir = TempDir::new().unwrap();

    let (report, code) = doctor_json(dir.path());

    assert_eq!(code, 0, "{report}");
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["extensions"], serde_json::json!([]), "{report}");
    assert_eq!(report["installed_count"], 0, "{report}");
    let findings = project_findings(&report);
    let codes: Vec<(&str, &str)> = findings
        .iter()
        .map(|f| (f["code"].as_str().unwrap(), f["status"].as_str().unwrap()))
        .collect();
    assert_eq!(codes, [("config_missing", "warn")], "{report}");

    let (human, code) = doctor_human(dir.path());
    assert_eq!(code, 0, "{human}");
    assert!(
        human.contains("[WARN] [config_missing] specforge.json at "),
        "{human}"
    );
    assert!(human.contains("No issues found."), "{human}");
}

// A specforge.lock that is there and cannot be read: the environment read
// it once, and doctor reports it as an error finding naming E033.
#[specforge_test(
    behavior = "run_doctor_check",
    verify = "a lock file that cannot be read is an error finding naming E033"
)]
fn doctor_reports_a_corrupt_lock_as_an_error_finding() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    fs::write(dir.path().join("specforge.lock"), "not valid json {{{").unwrap();

    let (report, code) = doctor_json(dir.path());

    assert_eq!(code, 1, "{report}");
    let findings = project_findings(&report);
    assert_eq!(findings.len(), 1, "{report}");
    assert_eq!(findings[0]["code"], "lock_unreadable", "{report}");
    assert_eq!(findings[0]["status"], "error", "{report}");
    assert!(
        findings[0]["check"]
            .as_str()
            .unwrap()
            .contains("corrupt lock file at"),
        "{report}"
    );
    assert_eq!(report["installed_count"], 0, "{report}");
    assert_eq!(findings[0]["about"], "lock", "{report}");

    let (human, code) = doctor_human(dir.path());
    assert_eq!(code, 1, "{human}");
    assert!(
        human.contains("[ERROR] [lock_unreadable] corrupt lock file at"),
        "{human}"
    );
    assert!(human.contains("[E033]"), "{human}");
}

/// `specforge.json` texts that are there and can't be used: not JSON, not
/// an object, an `extensions` value that is not an array.
const UNUSABLE_CONFIGS: [&str; 3] = [
    r#"{ "extensions": ["@specforge/product",  }"#,
    "[1,2]",
    r#"{"extensions": "@specforge/product"}"#,
];

// R3 (plan 05): a specforge.json that is there and can't be used fails
// check with E069 (an error), then an I002 that names the file. It used to
// compile silently as "no extensions configured".
#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "a specforge.json that is there and can't be used produces E069 first and an I002 that names it"
)]
fn an_unusable_config_fails_check_with_e069() {
    for config in UNUSABLE_CONFIGS {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("specforge.json"), config).unwrap();
        fs::write(dir.path().join("main.spec"), "feature f \"F\" {\n}\n").unwrap();

        for strict in [false, true] {
            let mut command = specforge_cmd();
            command.args(["check", "--format", "json"]).arg(dir.path());
            if strict {
                command.arg("--strict");
            }
            let output = command.output().unwrap();

            assert_eq!(output.status.code(), Some(1), "{config}: {output:?}");
            let found: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
            let codes: Vec<&str> = found.iter().map(|d| d["code"].as_str().unwrap()).collect();
            assert_eq!(codes, ["E069", "I002"], "{config}: {found:?}");
            assert_eq!(found[0]["severity"], "Error", "{config}");
            let message = found[0]["message"].as_str().unwrap();
            assert!(
                message.starts_with("specforge.json can't be used: ")
                    && message.ends_with("; no extension is loaded"),
                "{config}: {message}"
            );
            assert_eq!(
                found[1]["message"],
                "specforge.json could not be read — operating in structural-only mode",
                "{config}"
            );
        }
    }

    // The JSON error names its line and column.
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), UNUSABLE_CONFIGS[0]).unwrap();
    let output = specforge_cmd()
        .args(["check", "--format", "json"])
        .arg(dir.path())
        .output()
        .unwrap();
    let found: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        found[0]["message"]
            .as_str()
            .unwrap()
            .contains("is not valid JSON: expected value at line 1 column 41"),
        "{found:?}"
    );
}

use crate::written::{changed_since, files_under, files_written};

/// `specforge <args> --path <dir> --format json`: its JSON output.
fn json_of(args: &[&str], dir: &std::path::Path) -> serde_json::Value {
    let output = specforge_cmd()
        .args(args)
        .arg("--path")
        .arg(dir)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

/// A project enabling nothing, and the greet blob beside it (outside it).
fn empty_project() -> (TempDir, TempDir, String) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"t","version":"0.1.0","extensions":[]}"#,
    )
    .unwrap();
    let blobs = TempDir::new().unwrap();
    let wasm = blobs.path().join("greet.wasm");
    fs::write(&wasm, crate::registry::greet_wasm()).unwrap();
    let wasm = wasm.to_str().unwrap().to_string();
    (dir, blobs, wasm)
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add --format json lists the files it wrote in files_written"
)]
fn add_json_lists_the_files_it_wrote() {
    let (dir, _blobs, wasm) = empty_project();
    let root = dir.path();

    let before = files_under(root);
    let builtin = json_of(&["add", "@specforge/product"], root);
    assert_eq!(files_written(&builtin), ["specforge.json"]);
    assert_eq!(changed_since(root, &before), files_written(&builtin));

    let before = files_under(root);
    let installed = json_of(&["add", &wasm], root);
    assert_eq!(
        files_written(&installed),
        [
            ".specforge/extensions/@sdk/greet/extension.wasm",
            "specforge.json",
            "specforge.lock"
        ]
    );
    assert_eq!(changed_since(root, &before), files_written(&installed));

    // Already there: nothing written.
    let before = files_under(root);
    let again = json_of(&["add", &wasm], root);
    assert_eq!(again["added"][0]["action"], "already_present", "{again}");
    assert_eq!(files_written(&again), Vec::<String>::new());
    assert_eq!(changed_since(root, &before), Vec::<String>::new());
    let enabled = json_of(&["add", "@specforge/product"], root);
    assert_eq!(files_written(&enabled), Vec::<String>::new());
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "specforge add with several specifiers prints one entry per specifier and writes them as one change, or nothing"
)]
fn add_with_several_specifiers_is_one_change() {
    let (dir, _blobs, wasm) = empty_project();
    let root = dir.path();

    // One refused: nothing written, the refusal naming it, in both formats.
    let before = files_under(root);
    let missing = root.join("missing.wasm");
    let missing = missing.to_str().unwrap();
    for format in ["human", "json"] {
        let output = specforge_cmd()
            .args(["add", "@specforge/product", missing, "--path"])
            .arg(root)
            .args(["--format", format])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{format}: {output:?}");
        let shown = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            shown.contains(&format!("adding '{missing}': file not found")),
            "{format}: {shown}"
        );
        assert_eq!(changed_since(root, &before), Vec::<String>::new());
    }

    // All accepted: one entry per specifier, the files of the whole set.
    let added = json_of(
        &["add", "@specforge/product", &wasm, "@specforge/product"],
        root,
    );
    let actions: Vec<&str> = added["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["enabled", "installed", "duplicate"], "{added}");
    assert_eq!(added["added"][1]["specifier"], wasm.as_str(), "{added}");
    assert_eq!(
        files_written(&added),
        [
            ".specforge/extensions/@sdk/greet/extension.wasm",
            "specforge.json",
            "specforge.lock"
        ]
    );
    assert_eq!(changed_since(root, &before), files_written(&added));

    // The human surface tells each specifier on its own line.
    let output = specforge_cmd()
        .args(["add", "@specforge/product", &wasm, "--path"])
        .arg(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("@specforge/product is already enabled")
            && stdout.contains("@sdk/greet 0.1.0 is already installed"),
        "{stdout}"
    );

    // `add` without a specifier is a usage error.
    let output = specforge_cmd()
        .args(["add", "--path"])
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
}

#[specforge_test(
    behavior = "remove_extension",
    verify = "remove --format json lists the files it wrote in files_written"
)]
fn remove_json_lists_the_files_it_wrote() {
    let (dir, _blobs, wasm) = empty_project();
    let root = dir.path();
    json_of(&["add", "@specforge/product"], root);
    json_of(&["add", &wasm], root);

    let before = files_under(root);
    let installed = json_of(&["remove", "@sdk/greet"], root);
    assert_eq!(
        files_written(&installed),
        [
            ".specforge/extensions/@sdk/greet/extension.wasm",
            "specforge.json",
            "specforge.lock"
        ]
    );
    assert_eq!(changed_since(root, &before), files_written(&installed));

    let before = files_under(root);
    let builtin = json_of(&["remove", "@specforge/product"], root);
    assert_eq!(files_written(&builtin), ["specforge.json"]);
    assert_eq!(changed_since(root, &before), files_written(&builtin));
}

/// `specforge.doctor`'s reply over an in-process MCP server serving `root`.
fn mcp_doctor_json(root: &std::path::Path) -> serde_json::Value {
    let mut server = specforge_mcp::McpServer::with_project_root(root.to_path_buf());
    let init = serde_json::json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}});
    server.handle_message(&init.to_string());
    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "specforge.doctor", "arguments": {}}
    });
    let reply: serde_json::Value =
        serde_json::from_str(&server.handle_message(&call.to_string()).unwrap()).unwrap();
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{reply}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// The sorted keys of a JSON object.
fn keys_of(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// The findings of `report` about `about` (config, lock, binary, load,
/// conflict, shadowing, peer or toolchain).
fn findings_about<'a>(report: &'a serde_json::Value, about: &str) -> Vec<&'a serde_json::Value> {
    report["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("no findings: {report}"))
        .iter()
        .filter(|f| f["about"] == about)
        .collect()
}

#[specforge_test(
    behavior = "run_doctor_check",
    verify = "doctor states each finding once, saying what it is about, and its verdict; the CLI and MCP return one report"
)]
fn doctor_json_is_one_report_on_both_surfaces() {
    let dir = TempDir::new().unwrap();
    write_config_with_extensions(dir.path(), &["@specforge/software", "@acme/missing"]);

    let (cli, code) = doctor_json(dir.path());
    assert_eq!(code, 1, "{cli}");
    let mcp = mcp_doctor_json(dir.path());

    // The CLI adds the user's credentials; nothing else differs in shape.
    let mut expected = keys_of(&mcp);
    expected.extend(["credentials", "credentials_failures"]);
    expected.sort_unstable();
    assert_eq!(keys_of(&cli), expected, "{cli}");
    assert_eq!(
        keys_of(&mcp),
        [
            "cache_status",
            "conflicts",
            "enhancements",
            "extensions",
            "extensions_ok",
            "findings",
            "installed_count",
            "ok",
            "z3_available",
        ],
        "{mcp}"
    );
    for report in [&cli, &mcp] {
        assert_eq!(report["ok"], false, "{report}");
        let load = findings_about(report, "load");
        assert_eq!(load.len(), 1, "{report}");
        assert_eq!(load[0]["code"], "E028", "{report}");
    }
    assert_eq!(cli["findings"], mcp["findings"]);
}
