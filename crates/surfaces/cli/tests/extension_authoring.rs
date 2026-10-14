//! `specforge extension init|build|validate` author an extension with the
//! SDK: an SDK crate declaring the extension, its wasm32-wasip2 component,
//! and the declaration that component serves (ADR 0012).

use crate::support::{Cli, TestProject};
use specforge_repo_root::repo_root;
use specforge_test_macros::test as specforge_test;
use std::fs;
use std::path::{Path, PathBuf};

/// Every file under `dir`, recursively.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

/// An empty directory the test scaffolds and builds in (no `specforge.json`).
fn scratch() -> TestProject {
    TestProject::new().without_config()
}

/// `extension init --name <name>` in a fresh directory.
fn init(name: &str) -> (TestProject, PathBuf) {
    let dir = scratch();
    Cli::on(&dir)
        .bare_command(&["extension", "init", "--name", name, "--dir"])
        .arg(dir.root())
        .assert()
        .success();
    let ext = dir.root().join(name);
    (dir, ext)
}

fn json_of(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// Whether this toolchain can build wasm32-wasip2 components.
fn wasip2_installed() -> bool {
    std::process::Command::new("rustc")
        .args(["--print", "target-libdir", "--target", "wasm32-wasip2"])
        .output()
        .is_ok_and(|o| {
            o.status.success() && Path::new(String::from_utf8_lossy(&o.stdout).trim()).exists()
        })
}

// ===============================================================
// extension init
// ===============================================================

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "scaffold creates an SDK crate declaring the extension"
)]
fn extension_init_creates_an_sdk_crate() {
    let (_dir, ext) = init("my-ext");
    let lib = fs::read_to_string(ext.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("#[specforge_extension_sdk::extension("),
        "{lib}"
    );
    assert!(lib.contains(r#"name = "@local/my-ext""#), "{lib}");
    assert!(lib.contains(r#"short = "my-ext""#), "{lib}");
    assert!(lib.contains("description = "), "{lib}");
    let cargo = fs::read_to_string(ext.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("specforge-extension-sdk"), "{cargo}");
    assert!(cargo.contains(r#"crate-type = ["cdylib"]"#), "{cargo}");
    // The binary declares the extension: the crate is all there is.
    let mut written: Vec<String> = walk(&ext)
        .into_iter()
        .map(|p| {
            p.strip_prefix(&ext)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    written.sort();
    assert_eq!(written, [".cargo/config.toml", "Cargo.toml", "src/lib.rs"]);
}

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "scaffold creates src/ with skeleton exports"
)]
fn extension_init_creates_src_lib_rs() {
    let (_dir, ext) = init("skeleton-ext");
    let lib = fs::read_to_string(ext.join("src/lib.rs")).unwrap();
    // The SDK generates every export: the protocol's and the declared
    // command's.
    assert!(lib.contains("impl Contributions for Extension"), "{lib}");
    assert!(lib.contains("c.command(\"things\""), "{lib}");
    assert!(
        lib.contains(
            "specforge_extension_sdk::component_guest!(build = specforge_extension_build)"
        ),
        "{lib}"
    );
    assert!(!lib.contains("_start"), "{lib}");
}

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "scaffold builds for wasm32-wasip2"
)]
fn extension_init_targets_wasm32_wasip2() {
    let (_dir, ext) = init("target-ext");
    let config = fs::read_to_string(ext.join(".cargo/config.toml")).unwrap();
    assert!(config.contains(r#"target = "wasm32-wasip2""#), "{config}");
    let lib = fs::read_to_string(ext.join("src/lib.rs")).unwrap();
    assert!(lib.contains("wasm32-wasip2"), "{lib}");
    assert!(!lib.contains("wasip1"), "{lib}");
}

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "specforge extension init rejects when directory already exists"
)]
fn extension_init_rejects_existing_directory() {
    let dir = scratch();
    fs::create_dir(dir.root().join("taken")).unwrap();
    let output = Cli::on(&dir)
        .bare_command(&[
            "extension",
            "init",
            "--name",
            "taken",
            "--format",
            "json",
            "--dir",
        ])
        .arg(dir.root())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json_of(&output)["code"], "E065");
}

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "specforge extension init --format=json outputs structured JSON"
)]
fn extension_init_json_output() {
    let dir = scratch();
    let output = Cli::on(&dir)
        .bare_command(&[
            "extension",
            "init",
            "--name",
            "json-ext",
            "--format",
            "json",
            "--dir",
        ])
        .arg(dir.root())
        .output()
        .unwrap();
    assert!(output.status.success());
    let json = json_of(&output);
    assert_eq!(json["status"], "created");
    assert_eq!(json["name"], "@local/json-ext");
    assert_eq!(json["short"], "json-ext");
    assert_eq!(
        json["files"],
        serde_json::json!(["Cargo.toml", ".cargo/config.toml", "src/lib.rs"])
    );
}

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "specforge extension init uses default name when --name not provided"
)]
fn extension_init_default_name() {
    let dir = scratch();
    Cli::on(&dir)
        .bare_command(&["extension", "init", "--dir"])
        .arg(dir.root())
        .assert()
        .success();
    assert!(dir.root().join("my-extension/src/lib.rs").exists());
}

#[test]
fn the_next_steps_name_the_crate_with_dir() {
    let dir = scratch();
    let output = Cli::on(&dir)
        .bare_command(&["extension", "init", "--dir"])
        .arg(dir.root())
        .output()
        .unwrap();
    assert!(output.status.success());
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(shown.contains("specforge extension build --dir"), "{shown}");
    assert!(
        shown.contains("specforge extension validate --dir"),
        "{shown}"
    );
}

// ===============================================================
// extension build
// ===============================================================

#[specforge_test(
    behavior = "build_wasm_extension",
    verify = "specforge extension build validates project structure exists"
)]
fn extension_build_validates_structure() {
    let dir = scratch();
    let output = Cli::on(&dir)
        .with_toolchain()
        .bare_command(&["extension", "build", "--format", "json", "--dir"])
        .arg(dir.root())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json = json_of(&output);
    assert_eq!(json["code"], "E040", "{json}");
    assert_eq!(
        json["error"],
        format!("no Cargo.toml found at {}", dir.root().display())
    );
}

#[specforge_test(
    behavior = "build_wasm_extension",
    verify = "build errors reported as ExtensionError diagnostics"
)]
fn extension_build_reports_a_failed_build() {
    let dir = scratch();
    fs::write(
        dir.root().join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n",
    )
    .unwrap();
    fs::create_dir(dir.root().join("src")).unwrap();
    fs::write(dir.root().join("src/lib.rs"), "this is not rust").unwrap();
    let output = Cli::on(&dir)
        .with_toolchain()
        .bare_command(&["extension", "build", "--format", "json", "--dir"])
        .arg(dir.root())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json = json_of(&output);
    assert_eq!(json["code"], "E040", "{json}");
    let error = json["error"].as_str().unwrap();
    assert!(
        error.contains("cargo build --release --target wasm32-wasip2 failed"),
        "{error}"
    );
}

/// `init`, then `build`, then `validate`: the scaffold builds, as is, into
/// a component that declares a valid extension. Needs the wasm32-wasip2
/// target (CI installs it); without it, the build is skipped.
#[specforge_test(
    behavior = "build_wasm_extension",
    verify = "build produces .wasm binary"
)]
fn a_scaffolded_extension_builds_and_validates() {
    if !wasip2_installed() {
        eprintln!("skipped: the wasm32-wasip2 target is not installed");
        return;
    }
    let (dir, ext) = init("round-trip");
    // Build against this repository's SDK, from the local cargo cache.
    let mut cargo = fs::read_to_string(ext.join("Cargo.toml")).unwrap();
    let sdk = repo_root().join("crates/runtime/extension-sdk");
    cargo.push_str(&format!(
        "\n[patch.crates-io]\nspecforge-extension-sdk = {{ path = {:?} }}\n",
        sdk.canonicalize().unwrap()
    ));
    fs::write(ext.join("Cargo.toml"), cargo).unwrap();

    // A target directory configured elsewhere doesn't move the component
    // out of the crate, where `validate` and `publish` look.
    let elsewhere = tempfile::TempDir::new().unwrap();
    let output = Cli::on(&dir)
        .with_toolchain()
        .bare_command(&["extension", "build", "--format", "json", "--dir"])
        .arg(&ext)
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_TARGET_DIR", elsewhere.path())
        .output()
        .unwrap();
    let json = json_of(&output);
    assert!(output.status.success(), "{json}");
    assert_eq!(json["status"], "built");
    let component = PathBuf::from(json["component"].as_str().unwrap());
    assert!(component.ends_with("target/wasm32-wasip2/release/round_trip.wasm"));
    assert!(component.exists());

    let output = Cli::on(&dir)
        .bare_command(&["extension", "validate", "--format", "json", "--dir"])
        .arg(&ext)
        .output()
        .unwrap();
    let json = json_of(&output);
    assert!(output.status.success(), "{json}");
    assert_eq!(json["valid"], true);
    assert_eq!(json["name"], "@local/round-trip");
    assert_eq!(json["short"], "round-trip");
    assert_eq!(json["declaration"]["entities"][0]["name"], "thing");
}

// ===============================================================
// extension validate
// ===============================================================

fn greet_wasm() -> PathBuf {
    specforge_component::committed::GREET.path()
}

#[specforge_test(
    behavior = "validate_wasm_extension_locally",
    verify = "specforge extension validate errors when no built component is found"
)]
fn extension_validate_without_a_built_component() {
    let dir = scratch();
    let output = Cli::on(&dir)
        .bare_command(&["extension", "validate", "--format", "json", "--dir"])
        .arg(dir.root())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json = json_of(&output);
    assert_eq!(json["code"], "E040", "{json}");
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("no built component"),
        "{json}"
    );
}

#[specforge_test(
    behavior = "validate_wasm_extension_locally",
    verify = "specforge extension validate reports the declaration's registry build diagnostics"
)]
fn extension_validate_reports_the_registry_build_diagnostics() {
    // The formal builtin, alone: it enhances software's kinds, which no
    // loaded extension declares (I004), reported; its missing peer is not
    // (it is installed beside it), and it is still valid.
    let dir = scratch();
    let formal = repo_root().join("extensions/formal/wasm/specforge_ext_formal.wasm");
    let output = Cli::on(&dir)
        .bare_command(&["extension", "validate", "--format", "json", "--dir"])
        .arg(&formal)
        .output()
        .unwrap();
    let json = json_of(&output);
    assert!(output.status.success(), "{json}");
    assert_eq!(json["name"], "@specforge/formal");
    let codes: Vec<&str> = json["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"I004"), "{codes:?}");
    assert!(!codes.contains(&"E027"), "{codes:?}");

    // A binary that is not an extension is E028.
    let bogus = dir.root().join("bogus.wasm");
    fs::write(&bogus, b"\0asm\x01\0\0\0").unwrap();
    let output = Cli::on(&dir)
        .bare_command(&["extension", "validate", "--format", "json", "--dir"])
        .arg(&bogus)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json_of(&output)["code"], "E028");
}

#[specforge_test(
    behavior = "validate_extension_manifest",
    verify = "valid manifest passes validation"
)]
fn extension_validate_a_valid_declaration() {
    let project = scratch();
    let output = Cli::on(&project)
        .bare_command(&["extension", "validate", "--format", "json", "--dir"])
        .arg(greet_wasm())
        .output()
        .unwrap();
    let json = json_of(&output);
    assert!(output.status.success(), "{json}");
    assert_eq!(json["valid"], true);
    assert_eq!(json["name"], "@sdk/greet");
    assert_eq!(json["version"], "0.1.0");
    assert_eq!(json["short"], "greet");
    assert_eq!(json["diagnostics"], serde_json::json!([]));
    assert_eq!(json["declaration"]["handshake"]["name"], "@sdk/greet");

    // Human output says so.
    Cli::on(&project)
        .bare_command(&["extension", "validate", "--dir"])
        .arg(greet_wasm())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "@sdk/greet v0.1.0 declares a valid extension",
        ));
}

// ===============================================================
// Contract tests
// ===============================================================

#[specforge_test(
    behavior = "scaffold_wasm_extension_project",
    verify = "Scaffold Wasm Extension Project: Wasm extension scaffolding holds — filesystem_available, declaration_created, skeleton_exports_created, build_target_configured, extension_project_scaffolded_emitted"
)]
fn contract_init_creates_the_sdk_crate() {
    let (_dir, ext) = init("contract-ext");
    for file in ["Cargo.toml", ".cargo/config.toml", "src/lib.rs"] {
        assert!(ext.join(file).exists(), "{file}");
    }
    let lib = fs::read_to_string(ext.join("src/lib.rs")).unwrap();
    assert!(lib.contains(r#"name = "@local/contract-ext""#), "{lib}");
}

#[specforge_test(
    behavior = "build_wasm_extension",
    verify = "Build Wasm Extension: Wasm extension building holds — source_available, toolchain_available, wasm_binary_produced, build_errors_diagnosed, extension_built_emitted"
)]
fn contract_build_needs_a_crate() {
    // No crate: E040 before anything runs, on stderr in human form.
    let dir = scratch();
    Cli::on(&dir)
        .bare_command(&["extension", "build", "--dir"])
        .arg(dir.root())
        .assert()
        .code(1)
        .stderr(predicates::str::contains(
            "error[E040]: no Cargo.toml found",
        ));
}

#[specforge_test(
    behavior = "validate_extension_manifest",
    verify = "Validate Extension Manifest: extension declaration validation holds — declaration_loaded_fired, manifest_validated_emitted, invalid_manifest_diagnosed, schema_validated"
)]
fn contract_validate_exit_codes() {
    // A valid declaration exits 0, no component or a binary that is not an
    // extension exits 1.
    let dir = scratch();
    Cli::on(&dir)
        .bare_command(&["extension", "validate", "--dir"])
        .arg(greet_wasm())
        .assert()
        .success();
    Cli::on(&dir)
        .bare_command(&["extension", "validate", "--dir"])
        .arg(dir.root())
        .assert()
        .failure();
}
