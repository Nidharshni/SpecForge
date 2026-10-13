use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;
use std::fs;

/// An empty directory: no `specforge.json`, the project `init` scaffolds in.
fn scratch() -> TestProject {
    TestProject::new().without_config()
}

// ═══════════════════════════════════════════════════════════
// Behavior: scaffold_new_project
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffold creates valid specforge.json"
)]
fn init_creates_valid_specforge_json() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "my-project"])
        .assert()
        .success();

    let config_path = dir.root().join("specforge.json");
    assert!(config_path.exists(), "specforge.json should be created");

    let content = fs::read_to_string(&config_path).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("specforge.json should be valid JSON");

    assert_eq!(json["name"], "my-project");
    assert_eq!(json["version"], "0.1.0");
    assert!(json["extensions"].is_array());
    assert!(json["spec_root"].is_string());
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffold includes $schema field in generated config"
)]
fn init_includes_schema_field() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "test-schema"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert!(
        json["$schema"].is_string(),
        "specforge.json should include $schema field"
    );
    assert!(
        json["$schema"].as_str().unwrap().contains("specforge"),
        "$schema URL should reference specforge"
    );
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffold rejects when specforge.json already exists"
)]
fn init_rejects_existing_project() {
    let dir = scratch();
    fs::write(dir.root().join("specforge.json"), "{}").unwrap();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "duplicate"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffold inside another project creates a separate project"
)]
fn init_inside_another_project_creates_a_separate_one() {
    let dir = scratch();
    fs::write(dir.root().join("specforge.json"), "{}").unwrap();

    let child = dir.root().join("subdir");
    fs::create_dir_all(&child).unwrap();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "nested"])
        .current_dir(&child)
        .assert()
        .success()
        .stderr(predicates::str::contains("is inside the project at"));
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(child.join("specforge.json")).unwrap()).unwrap();
    assert_eq!(config["name"], "nested");

    // Commands run inside it resolve to the nearest project.
    Cli::on(&dir)
        .bare_command(&["check"])
        .current_dir(&child)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.root().join("specforge.json")).unwrap(),
        "{}",
        "the enclosing project is untouched"
    );
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffold in non-empty directory preserves existing files"
)]
fn init_preserves_existing_files() {
    let dir = scratch();
    fs::write(dir.root().join("README.md"), "# Hello").unwrap();
    fs::create_dir_all(dir.root().join("src")).unwrap();
    fs::write(dir.root().join("src/main.rs"), "fn main() {}").unwrap();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "existing"])
        .assert()
        .success();

    // Original files should still exist
    assert_eq!(
        fs::read_to_string(dir.root().join("README.md")).unwrap(),
        "# Hello"
    );
    assert_eq!(
        fs::read_to_string(dir.root().join("src/main.rs")).unwrap(),
        "fn main() {}"
    );
    // And specforge.json should also exist
    assert!(dir.root().join("specforge.json").exists());
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffold in non-empty directory preserves existing files"
)]
fn init_refuses_to_overwrite_a_starter_file() {
    let dir = scratch();
    fs::create_dir_all(dir.root().join("spec")).unwrap();
    fs::write(
        dir.root().join("spec/hello.spec"),
        "term mine \"Mine\" {\n}\n",
    )
    .unwrap();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "victim"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("spec/hello.spec already exists"));

    assert_eq!(
        fs::read_to_string(dir.root().join("spec/hello.spec")).unwrap(),
        "term mine \"Mine\" {\n}\n"
    );
    assert!(!dir.root().join("specforge.json").exists());
}

// ═══════════════════════════════════════════════════════════
// Behavior: scaffold_starter_spec_file
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "starter spec file is created alongside specforge.json"
)]
fn init_creates_starter_spec_file() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "test-starter"])
        .assert()
        .success();

    let starter = dir.root().join("spec").join("hello.spec");
    assert!(
        starter.exists(),
        "starter spec file should be created at spec/hello.spec"
    );
    let content = fs::read_to_string(&starter).unwrap();
    assert!(!content.is_empty(), "starter spec file should not be empty");
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "a freshly initialised project passes format --check and check"
)]
fn a_fresh_project_is_formatted_and_checks_clean() {
    // No extension, then each builtin alone: init enables the builtins it
    // requires.
    let builtins = specforge_component::builtins::BUILTIN_EXTENSIONS
        .iter()
        .map(|(name, _)| Some(*name));
    for extension in std::iter::once(None).chain(builtins) {
        let dir = scratch();
        let mut init = Cli::on(&dir).bare_command(&[]);
        init.args(["init", "--name", "fresh"])
            .current_dir(dir.root());
        if let Some(extension) = extension {
            init.args(["--extensions", extension]);
        }
        init.assert().success();

        for command in [&["format", "--check"][..], &["check"][..]] {
            let run = Cli::on(&dir)
                .bare_command(&[])
                .args(command)
                .current_dir(dir.root())
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "{command:?} after init with {extension:?}:\n{}{}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr),
            );
        }
    }
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "init with a builtin that requires another passes check"
)]
fn init_with_a_builtin_that_requires_another_passes_check() {
    let dir = scratch();

    let init = Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "needs-software",
            "--extensions",
            "@specforge/formal",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(init.status.success(), "{init:?}");
    let output: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    let installed: Vec<&str> = output["extensions_installed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .collect();
    let position = |name: &str| installed.iter().position(|e| *e == name);
    assert!(
        position("@specforge/software") < position("@specforge/formal")
            && position("@specforge/software").is_some(),
        "{installed:?}"
    );

    let check = Cli::on(&dir)
        .bare_command(&["check", "--format", "json"])
        .output()
        .unwrap();
    assert!(check.status.success(), "{check:?}");
    let reported = String::from_utf8_lossy(&check.stdout);
    assert!(!reported.contains("E027"), "{reported}");
}

#[test]
fn init_help_states_the_default_version_from_the_constant() {
    let help = Cli::on(&scratch())
        .bare_command(&["init", "-h"])
        .output()
        .unwrap();
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(
        text.contains(&format!(
            "[default: {}]",
            specforge_ops::init::DEFAULT_VERSION
        )),
        "{text}"
    );
    assert!(text.contains("[default: 0.1.0]"), "{text}");
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "starter spec file passes specforge check with zero errors"
)]
fn init_starter_passes_check() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "check-test"])
        .assert()
        .success();

    // Run specforge check on the newly created project
    Cli::on(&dir)
        .bare_command(&["check", "--format", "json", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "starter file uses only structural syntax when no extensions contribute templates"
)]
fn init_starter_uses_structural_syntax_only() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "structural"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("spec/hello.spec")).unwrap();

    // Should NOT contain domain-specific keywords from any extension
    let domain_keywords = [
        "behavior",
        "invariant",
        "feature",
        "event",
        "port",
        "journey",
        "deliverable",
        "milestone",
        "module",
        "decision",
        "constraint",
        "failure_mode",
        "term",
    ];
    for kw in domain_keywords {
        assert!(
            !content.contains(&format!("{kw} ")),
            "starter file should not contain domain keyword '{kw}'"
        );
    }
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "starter file content is deterministic for same extension set"
)]
fn init_starter_is_deterministic() {
    let dir1 = scratch();
    let dir2 = scratch();

    for dir in [dir1.root(), dir2.root()] {
        Cli::on(&dir1)
            .bare_command(&["init", "--name", "deterministic"])
            .current_dir(dir)
            .assert()
            .success();
    }

    let content1 = fs::read_to_string(dir1.root().join("spec/hello.spec")).unwrap();
    let content2 = fs::read_to_string(dir2.root().join("spec/hello.spec")).unwrap();

    assert_eq!(
        content1, content2,
        "same inputs should produce identical starter files"
    );
}

// ═══════════════════════════════════════════════════════════
// Behavior: graceful_zero_extension_init
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "zero-extension init creates valid specforge.json with empty extensions"
)]
fn zero_ext_init_creates_valid_config() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "zero-ext"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(json["extensions"], serde_json::json!([]));
}

#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "zero-extension config produces empty extensions array []"
)]
fn zero_ext_config_has_empty_array() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "empty-ext"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    // Verify the raw JSON contains "extensions": []
    assert!(
        content.contains(r#""extensions": []"#),
        "config should contain literal empty extensions array, got:\n{content}"
    );
}

#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "zero-extension starter file passes specforge check"
)]
fn zero_ext_starter_passes_check() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "zero-check"])
        .assert()
        .success();

    Cli::on(&dir)
        .bare_command(&["check", "--format", "json", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();
}

#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "zero-extension project produces valid graph via specforge export"
)]
fn zero_ext_export_produces_valid_graph() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "zero-export"])
        .assert()
        .success();

    let output = Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
        .output()
        .unwrap();

    assert!(output.status.success(), "export should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let graph: serde_json::Value =
        serde_json::from_str(&stdout).expect("export should produce valid JSON");
    assert!(graph["nodes"].is_array(), "graph should have nodes array");
    assert!(graph["edges"].is_array(), "graph should have edges array");
}

// ═══════════════════════════════════════════════════════════
// Behavior: non_interactive_init
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init creates valid specforge.json"
)]
fn non_interactive_creates_valid_config() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "ci-project"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["name"], "ci-project");
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init skips all prompts"
)]
fn non_interactive_no_prompts() {
    let dir = scratch();

    // Pipe empty stdin — should not hang waiting for input
    let output = Cli::on(&dir)
        .bare_command(&["init", "--name", "no-prompt"])
        .write_stdin("")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "should succeed without interactive input"
    );
    assert!(dir.root().join("specforge.json").exists());
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init with --extensions populates extensions list"
)]
fn non_interactive_with_extensions() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "ext-project",
            "--extensions",
            "@specforge/software",
            "--extensions",
            "@specforge/product",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    let extensions = json["extensions"].as_array().unwrap();
    // Enabling software enables the test vocabulary it relies on (ADR 0002).
    assert_eq!(extensions.len(), 3);
    assert_eq!(extensions[0], "@specforge/software");
    assert_eq!(extensions[1], "@specforge/product");
    assert_eq!(extensions[2], "@specforge/testing");
}

/// `init` enables the runner extensions for the test runners it finds: a
/// Cargo.toml, a vitest config, or vitest in package.json.
#[test]
fn init_enables_runner_extensions_for_detected_test_runners() {
    let init = |setup: &dyn Fn(&std::path::Path)| {
        let dir = scratch();
        setup(dir.root());
        Cli::on(&dir)
            .bare_command(&[
                "init",
                "--name",
                "demo",
                "--extensions",
                "@specforge/software",
            ])
            .assert()
            .success();
        let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        json["extensions"].clone()
    };
    let has =
        |exts: &serde_json::Value, name: &str| exts.as_array().unwrap().iter().any(|e| e == name);

    let none = init(&|_| {});
    assert!(!has(&none, "@specforge/cargo-test") && !has(&none, "@specforge/vitest"));

    let rust = init(&|d| fs::write(d.join("Cargo.toml"), "").unwrap());
    assert!(has(&rust, "@specforge/cargo-test") && !has(&rust, "@specforge/vitest"));

    let config = init(&|d| fs::write(d.join("vitest.config.ts"), "").unwrap());
    assert!(has(&config, "@specforge/vitest"));

    let dependency = init(&|d| {
        fs::write(
            d.join("package.json"),
            r#"{"devDependencies":{"vitest":"^5.0.0"}}"#,
        )
        .unwrap()
    });
    assert!(has(&dependency, "@specforge/vitest"));
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init with --format=json prints the init document specforge.init answers"
)]
fn non_interactive_json_output() {
    let dir = scratch();

    let output = Cli::on(&dir)
        .bare_command(&["init", "--name", "json-out", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("--format=json should produce valid JSON");

    assert!(json["project_root"].is_string(), "should have project_root");
    assert_eq!(json["config_file"], "specforge.json");
    assert_eq!(json["starter_file"], "spec/hello.spec");
    assert_eq!(json["name"], "json-out");
    assert!(json["version"].is_string(), "should have version");
    assert!(
        json["extensions_installed"].is_array(),
        "should have extensions_installed"
    );
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init --format=json includes project_root, name, version, config_file, starter_file and extensions_installed"
)]
fn non_interactive_json_all_fields() {
    let dir = scratch();

    let output = Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "fields-test",
            "--format",
            "json",
            "--extensions",
            "@specforge/software",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Every field of the init document, in its type.
    let project_root = json["project_root"].as_str().unwrap();
    assert!(std::path::Path::new(project_root).is_absolute());
    assert_eq!(json["name"], "fields-test");
    assert_eq!(json["version"], "0.1.0");
    assert_eq!(json["config_file"], "specforge.json");
    assert_eq!(json["starter_file"], "spec/hello.spec");

    let exts = json["extensions_installed"].as_array().unwrap();
    assert_eq!(
        exts,
        &[
            serde_json::json!("@specforge/software"),
            serde_json::json!("@specforge/testing")
        ]
    );
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init with --version overrides default version in specforge.json"
)]
fn non_interactive_version_override() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "versioned", "--version", "2.0.0"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["version"], "2.0.0");
}

#[test]
fn non_interactive_matches_default_name() {
    // With --name, the result should be deterministic regardless of how
    // the name was provided (flag vs. directory name inference).
    let dir1 = scratch();
    let dir2 = scratch();

    for dir in [dir1.root(), dir2.root()] {
        Cli::on(&dir1)
            .bare_command(&["init", "--name", "same-project"])
            .current_dir(dir)
            .assert()
            .success();
    }

    let c1 = fs::read_to_string(dir1.root().join("specforge.json")).unwrap();
    let c2 = fs::read_to_string(dir2.root().join("specforge.json")).unwrap();
    assert_eq!(c1, c2, "same --name should produce identical config");
}

// ═══════════════════════════════════════════════════════════
// Additional coverage: scaffold_new_project
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "scaffolded project passes init-check-export cycle"
)]
fn init_check_export_cycle() {
    let dir = scratch();

    // init
    Cli::on(&dir)
        .bare_command(&["init", "--name", "cycle-test"])
        .assert()
        .success();

    // check
    Cli::on(&dir)
        .bare_command(&["check", "--format", "json", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    // export
    let output = Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
        .output()
        .unwrap();
    assert!(output.status.success(), "export should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _graph: serde_json::Value =
        serde_json::from_str(&stdout).expect("export should produce valid JSON graph");
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "full init-check-export cycle completes in under 60 seconds"
)]
fn init_check_export_performance() {
    let dir = scratch();
    let start = std::time::Instant::now();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "perf-test"])
        .assert()
        .success();

    Cli::on(&dir)
        .bare_command(&["check", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 60,
        "init-check-export cycle took {}s, expected < 60s",
        elapsed.as_secs()
    );
}

// ═══════════════════════════════════════════════════════════
// Additional coverage: scaffold_starter_spec_file
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "starter file contains no domain-specific keywords from extensions"
)]
fn init_starter_no_domain_keywords() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "no-domain"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("spec/hello.spec")).unwrap();

    // All 14 domain entity keywords from the 3 standard extensions
    let domain_keywords = [
        "behavior",
        "invariant",
        "feature",
        "event",
        "port",
        "type",
        "journey",
        "deliverable",
        "milestone",
        "module",
        "term",
        "decision",
        "constraint",
        "failure_mode",
    ];
    for kw in domain_keywords {
        // Check for keyword used as entity declaration (keyword followed by space + id)
        let pattern = format!("\n{kw} ");
        assert!(
            !content.contains(&pattern),
            "starter file should not contain domain entity declaration '{kw}'"
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Additional coverage: graceful_zero_extension_init
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "graceful_zero_extension_init completes full init-check-export cycle in under 60 seconds"
)]
fn zero_ext_full_cycle_performance() {
    let dir = scratch();
    let start = std::time::Instant::now();

    // init with zero extensions (default)
    Cli::on(&dir)
        .bare_command(&["init", "--name", "zero-perf"])
        .assert()
        .success();

    // check
    Cli::on(&dir)
        .bare_command(&["check", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    // export
    Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 60,
        "zero-extension init-check-export cycle took {}s, expected < 60s",
        elapsed.as_secs()
    );
}

// ═══════════════════════════════════════════════════════════
// Additional coverage: non_interactive_init
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "invalid project name is rejected with InitError::invalid_name"
)]
fn non_interactive_invalid_name_rejected() {
    let dir = scratch();

    // Empty name
    Cli::on(&dir)
        .bare_command(&["init", "--name", ""])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid project name"));

    // Name starting with dot
    let dir2 = scratch();
    Cli::on(&dir)
        .bare_command(&["init", "--name", ".hidden"])
        .current_dir(dir2.root())
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid project name"));

    // Name starting with dash — use --name=VALUE form to prevent clap flag parsing
    let dir3 = scratch();
    Cli::on(&dir)
        .bare_command(&["init", "--name=-bad"])
        .current_dir(dir3.root())
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid project name"));
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non_interactive_init completes full init-check-export cycle in under 60 seconds"
)]
fn non_interactive_full_cycle_performance() {
    let dir = scratch();
    let start = std::time::Instant::now();

    Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "ci-perf",
            "--extensions",
            "@specforge/software",
        ])
        .assert()
        .success();

    Cli::on(&dir)
        .bare_command(&["check", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 60,
        "non-interactive init-check-export cycle took {}s, expected < 60s",
        elapsed.as_secs()
    );
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "non-interactive init with unknown extension rejects with diagnostic and exit code 1"
)]
fn non_interactive_unknown_extension_rejected() {
    let dir = scratch();

    // A well-formed name nothing can install: it used to pass the syntax
    // check, exit 0 and report the extension installed, and `check` then
    // failed E028. A new project has no registry: E063, as `add` answers it
    // with none configured, saying how to add it once there is one.
    Cli::on(&dir)
        .bare_command(&["init", "--name", "bad-ext", "--extensions", "@acme/unknown"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("error[E063]"))
        .stderr(predicates::str::contains("specforge add @acme/unknown"));

    // Nothing is written on failure.
    assert_eq!(std::fs::read_dir(dir.root()).unwrap().count(), 0);

    // A malformed one, too.
    Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "bad-ext",
            "--extensions",
            "not-a-valid-ext",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("error[E054]"));
    assert!(!dir.root().join("specforge.json").exists());

    // Empty extension
    let dir2 = scratch();
    Cli::on(&dir)
        .bare_command(&["init", "--name", "empty-ext", "--extensions", ""])
        .current_dir(dir2.root())
        .assert()
        .failure()
        .stderr(predicates::str::contains("error[E054]"))
        .stderr(predicates::str::contains("empty extension specifier"));
}

// ═══════════════════════════════════════════════════════════
// Contract tests (requires/ensures consistency)
// ═══════════════════════════════════════════════════════════

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "Scaffold New Project: new project scaffolding holds — filesystem_available, no_existing_project, valid_config_created, schema_field_included, project_initialized_emitted"
)]
fn scaffold_new_project_contract_in_init() {
    // Requires: filesystem available, no existing project
    // Ensures: valid config created, $schema included, project_initialized emitted
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "contract-scaffold"])
        .assert()
        .success();

    let config_path = dir.root().join("specforge.json");
    assert!(config_path.exists(), "specforge.json must be created");

    let content = fs::read_to_string(&config_path).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("specforge.json must be valid JSON");
    assert_eq!(json["name"], "contract-scaffold");
    assert!(json["version"].is_string(), "version must be present");
    assert!(json["$schema"].is_string(), "$schema must be present");
    assert!(json["extensions"].is_array(), "extensions must be an array");
    assert!(json["spec_root"].is_string(), "spec_root must be present");

    // Starter spec file must also exist
    assert!(dir.root().join("spec").join("hello.spec").exists());

    // Reject when project already exists (precondition violated)
    Cli::on(&dir)
        .bare_command(&["init", "--name", "contract-scaffold"])
        .assert()
        .failure();
}

/// The starter file `init --name demo` writes with `extensions` enabled.
fn starter_written_for(extensions: &[&str]) -> String {
    let dir = scratch();
    let mut cmd = Cli::on(&dir).bare_command(&[]);
    cmd.args(["init", "--name", "demo"]);
    for ext in extensions {
        cmd.args(["--extensions", ext]);
    }
    cmd.current_dir(dir.root()).assert().success();
    fs::read_to_string(dir.root().join("spec").join("hello.spec")).unwrap()
}

/// The starter template the builtin `extension` declares in its handshake,
/// with its `{project}` and `{version}` placeholders filled in for a project
/// named `demo` at the default version,
/// as the formatter writes it (init formats the starter it writes).
fn declared_starter(extension: &str) -> Option<String> {
    let runtime = specforge_component::ComponentRuntime::new();
    specforge_component::builtins::load_builtins_for(&runtime, &[extension.to_string()]).unwrap();
    let loaded = specforge_wasm::protocol::load_declaration(&runtime, extension).unwrap();
    loaded
        .declaration
        .handshake
        .starter_template
        .map(|template| {
            let template = template
                .replace("{project}", "demo")
                .replace("{version}", "0.1.0");
            specforge_formatter::format_source(
                &template,
                &specforge_formatter::FormatConfig::default(),
            )
            .formatted
        })
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "extension-contributed starter templates are used when available"
)]
fn starter_uses_extension_templates_when_available() {
    let structural = starter_written_for(&[]);
    for extension in ["@specforge/software", "@specforge/product"] {
        let declared = declared_starter(extension)
            .unwrap_or_else(|| panic!("{extension} declares a starter template"));
        assert_ne!(declared, structural, "{extension}");
        assert_eq!(starter_written_for(&[extension]), declared, "{extension}");
    }
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "when several enabled extensions contribute starter templates, the one listed first in specforge.json is used"
)]
fn first_listed_extension_template_wins() {
    let software = declared_starter("@specforge/software").unwrap();
    let product = declared_starter("@specforge/product").unwrap();
    assert_ne!(software, product);
    assert_eq!(
        starter_written_for(&["@specforge/software", "@specforge/product"]),
        software
    );
    assert_eq!(
        starter_written_for(&["@specforge/product", "@specforge/software"]),
        product
    );
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "starter file uses only structural syntax when no extensions contribute templates"
)]
fn extensions_without_a_template_get_the_structural_starter() {
    let structural = starter_written_for(&[]);
    assert_eq!(
        structural,
        "// demo — starter spec file\n\
         //\n\
         // This file uses only structural syntax that the core compiler\n\
         // understands without any extensions. Install extensions to unlock\n\
         // domain-specific entity types.\n\
         //\n\
         // Try: specforge check\n\
         \n\
         spec \"demo\" {\n  version \"0.1.0\"\n}\n"
    );
    for extension in ["@specforge/governance", "@specforge/testing"] {
        assert_eq!(declared_starter(extension), None, "{extension}");
        assert_eq!(starter_written_for(&[extension]), structural, "{extension}");
    }
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "extension-contributed starter file passes specforge check with zero errors"
)]
fn extension_starter_passes_check() {
    for extension in ["@specforge/software", "@specforge/product"] {
        let dir = scratch();
        Cli::on(&dir)
            .bare_command(&[])
            .args(["init", "--name", "ext-check", "--extensions", extension])
            .current_dir(dir.root())
            .assert()
            .success();

        // The starter an extension contributes must pass check in the
        // project it scaffolds.
        let output = Cli::on(&dir)
            .bare_command(&["check", "--format", "json"])
            .output()
            .unwrap();
        let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d["severity"] == "Error")
            .collect();
        assert!(errors.is_empty(), "{extension}: {errors:?}");
        assert_eq!(output.status.code(), Some(0), "{extension}");
    }
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "Scaffold Starter Spec File: starter spec file scaffolding holds — config_created, filesystem_available, starter_file_created, structural_syntax_only, zero_diagnostic_pass"
)]
fn scaffold_starter_spec_file_contract() {
    // Requires: specforge.json created, filesystem available
    // Ensures: starter file created, structural syntax only (no extensions),
    //          zero diagnostic pass
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "starter-contract"])
        .assert()
        .success();

    // Config must exist (precondition for starter file creation)
    assert!(dir.root().join("specforge.json").exists());

    // Starter file must exist alongside config
    let starter = dir.root().join("spec").join("hello.spec");
    assert!(starter.exists(), "starter file must be created");
    let content = fs::read_to_string(&starter).unwrap();
    assert!(!content.is_empty(), "starter file must not be empty");

    // Must pass check with zero diagnostics
    Cli::on(&dir)
        .bare_command(&["check", "--format", "json", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "Non-Interactive Init: non-interactive init holds — name_flag_provided, filesystem_available, no_existing_project, config_identical_to_interactive, all_prompts_skipped, json_output_supported, project_initialized_emitted"
)]
fn non_interactive_init_contract_in_init() {
    // Requires: --name flag provided, filesystem available, no existing project
    // Ensures: config identical to interactive, all prompts skipped,
    //          JSON output supported, project_initialized emitted
    let dir = scratch();

    // name_flag_provided, filesystem_available, all_prompts_skipped: with
    // --name and a closed stdin, init runs to completion.
    // project_initialized_emitted: it reports the initialized project.
    let output = Cli::on(&dir)
        .bare_command(&["init", "--name", "ni-contract"])
        .write_stdin("")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "must succeed without interactive input"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Initialized project 'ni-contract'"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );

    // config_identical_to_interactive: the same config as a plain `init`
    // in a directory of that name, which takes every default.
    let parent = scratch();
    let named = parent.root().join("ni-contract");
    fs::create_dir(&named).unwrap();
    Cli::on(&dir)
        .bare_command(&["init"])
        .current_dir(&named)
        .assert()
        .success();
    let config = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    assert_eq!(
        config,
        fs::read_to_string(named.join("specforge.json")).unwrap()
    );
    let json: serde_json::Value = serde_json::from_str(&config).unwrap();
    assert_eq!(json["name"], "ni-contract");

    // no_existing_project: init again in the same directory fails.
    Cli::on(&dir)
        .bare_command(&["init", "--name", "ni-contract"])
        .assert()
        .failure();
    assert_eq!(
        fs::read_to_string(dir.root().join("specforge.json")).unwrap(),
        config
    );

    // json_output_supported: --format json prints the init document.
    let json_dir = scratch();
    let output = Cli::on(&dir)
        .bare_command(&["init", "--name", "ni-json", "--format", "json"])
        .current_dir(json_dir.root())
        .output()
        .unwrap();
    assert!(output.status.success());
    let out: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let root = json_dir.root().canonicalize().unwrap();
    let root_of = std::path::PathBuf::from(out["project_root"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(root_of, root);
    assert_eq!(
        root.join(out["config_file"].as_str().unwrap()),
        root.join("specforge.json")
    );
    assert_eq!(
        root.join(out["starter_file"].as_str().unwrap()),
        root.join("spec").join("hello.spec")
    );
    assert_eq!(out["extensions_installed"], serde_json::json!([]));
}

#[specforge_test(
    behavior = "graceful_zero_extension_init",
    verify = "Graceful Zero-Extension Init: zero-extension init holds — zero_extensions_selected, filesystem_available, empty_extensions_list, structural_starter_valid, valid_graph_exportable, project_initialized_emitted"
)]
fn graceful_zero_extension_init_contract_in_init() {
    // Requires: zero extensions selected, filesystem available
    // Ensures: empty extensions list, structural starter valid,
    //          valid graph exportable, project_initialized emitted
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "zero-contract"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        json["extensions"],
        serde_json::json!([]),
        "extensions must be empty array"
    );

    // Check passes
    Cli::on(&dir)
        .bare_command(&["check", "--format", "json", "--path"])
        .arg(dir.root().join("spec"))
        .assert()
        .success();

    // Export produces valid graph
    let export_output = Cli::on(&dir)
        .bare_command(&["export", "--format", "graph", "--path"])
        .arg(dir.root().join("spec"))
        .output()
        .unwrap();
    assert!(export_output.status.success());
    let stdout = String::from_utf8_lossy(&export_output.stdout);
    let graph: serde_json::Value =
        serde_json::from_str(&stdout).expect("export must produce valid JSON");
    assert!(graph["nodes"].is_array());
    assert!(graph["edges"].is_array());
}

#[specforge_test(
    behavior = "scaffold_starter_spec_file",
    verify = "the software starter passes specforge check with no warnings"
)]
fn init_software_starter_checks_clean() {
    let dir = scratch();
    Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "demo",
            "--extensions",
            "@specforge/software",
        ])
        .assert()
        .success();
    Cli::on(&dir)
        .bare_command(&["check", "--format", "json"])
        .assert()
        .success();
}

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "init adds the generated report files to .gitignore without duplicating entries"
)]
fn init_ignores_generated_files() {
    let dir = scratch();
    fs::write(
        dir.root().join(".gitignore"),
        "/target\nspecforge-report.json",
    )
    .unwrap();
    Cli::on(&dir)
        .bare_command(&["init", "--name", "demo"])
        .assert()
        .success();
    let gitignore = fs::read_to_string(dir.root().join(".gitignore")).unwrap();
    assert_eq!(
        gitignore,
        "/target\nspecforge-report.json\nspecforge-infer.json\n.specforge/\n"
    );
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "--extensions splits a comma-separated list into its extensions"
)]
fn extensions_flag_splits_a_comma_separated_list() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "listed",
            "--extensions",
            "@specforge/software,@specforge/product",
        ])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.root().join("specforge.json")).unwrap())
            .unwrap();
    // Two entries (and software's testing), not one "a,b" entry.
    assert_eq!(
        config["extensions"],
        serde_json::json!([
            "@specforge/software",
            "@specforge/product",
            "@specforge/testing"
        ])
    );
    Cli::on(&dir).command(&["check"]).assert().success();
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "a one-character project name is rejected before its starter can fail E014"
)]
fn one_character_name_is_rejected() {
    let dir = scratch();

    Cli::on(&dir)
        .bare_command(&["init", "--name", "a"])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("invalid project name 'a'"));
    assert_eq!(std::fs::read_dir(dir.root()).unwrap().count(), 0);

    // Two characters is the shortest ID: its starter checks clean.
    Cli::on(&dir)
        .bare_command(&["init", "--name", "ab"])
        .assert()
        .success();
    Cli::on(&dir).command(&["check"]).assert().success();
}

use crate::support::written::{changed_since, files_written};

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "init lists every file it wrote, in its human and JSON output"
)]
fn init_lists_every_file_it_wrote() {
    // Human: one line per file written.
    let dir = scratch();
    let output = Cli::on(&dir)
        .bare_command(&["init", "--name", "listed"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let listed: Vec<&str> = stdout
        .lines()
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .map(str::trim)
        .collect();
    let written = changed_since(dir.root(), &Default::default());
    assert_eq!(written, [".gitignore", "spec/hello.spec", "specforge.json"]);
    assert_eq!(listed, written, "{stdout}");

    // JSON, with a local extension: its module and the lock too.
    let dir = scratch();
    let blob = scratch();
    let wasm = blob.root().join("greet.wasm");
    fs::write(&wasm, crate::support::registry::greet_wasm()).unwrap();
    let output = Cli::on(&dir)
        .bare_command(&[
            "init",
            "--name",
            "listed",
            "--format",
            "json",
            "--extensions",
        ])
        .arg(&wasm)
        .current_dir(dir.root())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let written = changed_since(dir.root(), &Default::default());
    assert_eq!(
        written,
        [
            ".gitignore",
            ".specforge/extensions/@sdk/greet/extension.wasm",
            "spec/hello.spec",
            "specforge.json",
            "specforge.lock",
        ]
    );
    assert_eq!(files_written(&json), written);
}
