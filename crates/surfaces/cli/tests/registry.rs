use crate::support::registry::{greet_named, greet_wasm, project_on};
use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

/// An empty directory: no `specforge.json`.
fn scratch() -> TestProject {
    TestProject::new().without_config()
}

#[test]
fn add_invalid_specifier_fails() {
    let dir = scratch();
    Cli::on(&dir)
        .command(&["add", "" /* an empty specifier */])
        .assert()
        .failure();
}

#[test]
fn add_local_nonexistent_file_fails() {
    let dir = TestProject::new().without_config();
    Cli::on(&dir)
        .command(&["add", "./nonexistent.wasm"])
        .assert()
        .failure();
}

#[test]
fn add_local_file_succeeds() {
    let dir = project_without_registry();

    let wasm_path = dir.root().join("test-ext.wasm");
    std::fs::write(&wasm_path, greet_wasm()).unwrap();

    Cli::on(&dir)
        .command(&["add"])
        .arg(wasm_path.to_str().unwrap())
        .assert()
        .success();

    // Verify lock file was created
    assert!(dir.root().join("specforge.lock").exists());
}

#[test]
fn add_local_file_json_output() {
    let dir = project_without_registry();

    let wasm_path = dir.root().join("my-ext.wasm");
    std::fs::write(&wasm_path, greet_wasm()).unwrap();

    let output = Cli::on(&dir)
        .command(&["add"])
        .arg(wasm_path.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["added"][0]["action"], "installed");
    assert_eq!(json["added"][0]["source"], "local:my-ext.wasm");
}

#[test]
fn publish_without_a_built_extension_fails() {
    let dir = TestProject::new().without_config();
    Cli::on(&dir).command(&["publish"]).assert().failure();
}

#[test]
fn publish_without_a_built_extension_json_output() {
    let dir = TestProject::new().without_config();
    let output = Cli::on(&dir)
        .command(&["publish", "--format", "json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "E040");
}

#[test]
fn update_without_lockfile_fails() {
    let dir = TestProject::new().without_config();
    Cli::on(&dir).command(&["update"]).assert().failure();
}

#[test]
fn logout_without_credentials_succeeds() {
    // A named registry is forgotten without reading any project.
    Cli::on(&scratch())
        .bare_command(&["logout", "--url", "https://plan06.invalid"])
        .assert()
        .success();
}

#[test]
fn login_without_token_fails() {
    let dir = TestProject::new().without_config();
    Cli::on(&dir).command(&["login"]).assert().failure();
}

// ===============================================================
// No built-in registry (ADR 0004 N1)
// ===============================================================
//
// SpecForge does not own `specforge.dev`, so it ships no default registry:
// with none configured, every registry command must fail with E063 before
// it touches the network. `NetSpy` proves "before": it is the proxy every
// HTTP client in the child is pointed at, and it counts the connections it
// receives.

/// A local listener set as the child's HTTP(S) proxy; it accepts and drops
/// every connection, counting them.
struct NetSpy {
    url: String,
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl NetSpy {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stream.is_ok() {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });
        NetSpy { url, hits }
    }

    /// The environment of a spied child: every proxy variable points at the
    /// spy. (HOME, with the credentials, keys and pins, is the project's own.)
    fn env(&self) -> Vec<(&'static str, String)> {
        let mut env: Vec<(&'static str, String)> = [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ]
        .into_iter()
        .map(|var| (var, self.url.clone()))
        .collect();
        env.push(("NO_PROXY", String::new()));
        env.push(("no_proxy", String::new()));
        env
    }

    /// `specforge` on `project` under the spy.
    fn on<'p>(&self, project: &'p TestProject) -> Cli<'p> {
        self.env()
            .into_iter()
            .fold(Cli::on(project), |cli, (key, value)| cli.env(key, value))
    }

    /// Connections received so far (after a moment for the accept loop).
    fn hits(&self) -> usize {
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// A project whose specforge.json configures no registry.
fn project_without_registry() -> TestProject {
    TestProject::new().config(|c| c["name"] = "demo".into())
}

/// The command failed with E063, and its hint names the `registries` key.
fn assert_no_registry(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "expected failure: {stdout}");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(json["code"], "E063", "{json}");
    let hint = json["suggestion"].as_str().unwrap_or_default();
    assert!(
        hint.contains("\"registries\"") && hint.contains("specforge.json"),
        "the hint must say how to configure a registry: {json}"
    );
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "with no registry configured, add makes no network call and reports how to configure one"
)]
fn add_without_registry_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    let output = spy
        .on(&dir)
        .command(&["add", "@acme/widget@1.0.0", "--format", "json"])
        .output()
        .unwrap();
    assert_no_registry(&output);
    assert_eq!(spy.hits(), 0, "add reached the network");
}

#[specforge_test(
    behavior = "search_registry",
    verify = "with no registry configured, search makes no network call and reports how to configure one"
)]
fn search_without_registry_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    let output = spy
        .on(&dir)
        .command(&["search", "widget", "--format", "json"])
        .output()
        .unwrap();
    assert_no_registry(&output);
    assert_eq!(spy.hits(), 0, "search reached the network");
}

#[specforge_test(
    behavior = "search_registry",
    verify = "an unusable specforge.json is refused with the refusal add gives, before any network call"
)]
fn search_and_login_refuse_an_unusable_config_as_add_does() {
    let spy = NetSpy::start();
    let dir = TestProject::new().raw_config("{ \"extensions\": [");
    let refusal = |args: &[&str]| -> (bool, serde_json::Value) {
        let mut argv = args.to_vec();
        argv.extend(["--format", "json"]);
        let output = spy.on(&dir).command(&argv).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
        (output.status.success(), json)
    };

    let (ok, added) = refusal(&["add", "@acme/widget@1.0.0"]);
    assert!(!ok);
    assert_eq!(added["code"], "config_invalid", "{added}");

    // (`publish` builds its declaration first; its registry read is the same
    // `configured`, covered in specforge-ops-registry.)
    for args in [&["search", "widget"][..], &["login", "--token", "t"]] {
        let (ok, json) = refusal(args);
        assert!(!ok, "{args:?}");
        assert_eq!(json["code"], added["code"], "{args:?}: {json}");
        assert_eq!(json["message"], added["message"], "{args:?}: {json}");
    }
    assert_eq!(spy.hits(), 0, "a command reached the network");
}

#[specforge_test(
    behavior = "update_all_extensions",
    verify = "with no registry configured, update makes no network call and reports how to configure one"
)]
fn update_without_registry_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    dir.write(
        "specforge.lock",
        r#"{"lockfile_version":1,"entries":[
            {"name":"@acme/widget","version":"1.0.0","source":"registry","wasm_hash":"00"}]}"#,
    );
    let output = spy
        .on(&dir)
        .command(&["update", "--format", "json"])
        .output()
        .unwrap();
    assert_no_registry(&output);
    assert_eq!(spy.hits(), 0, "update reached the network");
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "with no registry configured, publish makes no network call and reports how to configure one"
)]
fn publish_without_registry_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    dir.write("greet.wasm", greet_wasm());
    let output = spy
        .on(&dir)
        .command(&["publish", "--format", "json"])
        .arg(dir.root().join("greet.wasm"))
        .output()
        .unwrap();
    assert_no_registry(&output);
    assert_eq!(spy.hits(), 0, "publish reached the network");
}

#[test]
fn publish_with_no_credential_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = TestProject::new().raw_config(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": [],
            "registries": [{ "alias": "main", "url": UNREACHABLE, "default_registry": true }],
        })
        .to_string(),
    );
    dir.write("greet.wasm", greet_wasm());
    let output = spy
        .on(&dir)
        .command(&["publish", "--format", "json"])
        .arg(dir.root().join("greet.wasm"))
        .env_remove("SPECFORGE_REGISTRY_TOKEN")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "R001", "{json}");
    assert_eq!(spy.hits(), 0, "publish reached the network");
}

#[specforge_test(
    behavior = "resolve_registry_source",
    verify = "a name no registry serves is refused with R-OPS-001 before any request"
)]
fn publish_refuses_a_name_no_registry_serves_offline() {
    let spy = NetSpy::start();
    let dir = TestProject::new().raw_config(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": [],
            "registries": [{ "alias": "acme", "url": UNREACHABLE, "scope_filter": "@acme" }],
        })
        .to_string(),
    );
    dir.write("greet.wasm", greet_wasm());
    let output = spy
        .on(&dir)
        .command(&["publish", "--format", "json"])
        .arg(dir.root().join("greet.wasm"))
        .env("SPECFORGE_REGISTRY_TOKEN", "t")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "R-OPS-001", "{json}");
    assert!(
        json["suggestion"]
            .as_str()
            .unwrap()
            .contains("\"scope_filter\": \"@sdk\""),
        "{json}"
    );
    assert_eq!(spy.hits(), 0, "publish reached the network");
    assert!(
        !dir.home().join(".specforge/signing-key.json").exists(),
        "a refused publish created the signing key"
    );
}

#[specforge_test(
    behavior = "publish_to_registry",
    verify = "publish refuses in one order, each refusal before anything after it is read or asked"
)]
fn publish_checks_the_name_before_the_registry_configuration() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    dir.write("ext.wasm", greet_named("greet-ext1"));
    let output = spy
        .on(&dir)
        .command(&["publish", "--format", "json"])
        .arg(dir.root().join("ext.wasm"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "E072", "{json}");
    assert_eq!(spy.hits(), 0, "publish reached the network");
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "with no registry configured, login makes no network call and reports how to configure one"
)]
fn login_without_registry_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    let output = spy
        .on(&dir)
        .command(&["login", "--token", "t0k3n", "--format", "json"])
        .output()
        .unwrap();
    assert_no_registry(&output);
    assert_eq!(spy.hits(), 0, "login reached the network");
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "with no registry configured, add_extension makes no network call and reports how to configure one"
)]
fn mcp_add_extension_without_registry_makes_no_network_call() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    let requests = [
        crate::support::mcp_process::initialize(0),
        crate::support::mcp_process::request(
            1,
            "tools/call",
            serde_json::json!({
                "name": "specforge.add_extension",
                "arguments": {"specifiers": ["@acme/widget"]}}),
        ),
    ];
    let root = dir.root().to_str().unwrap();
    let responses =
        crate::support::mcp_process::session(&dir, dir.root(), root, &requests, &spy.env());
    let response = responses
        .iter()
        .find(|r| r["id"] == 1)
        .unwrap_or_else(|| panic!("no response to the call: {responses:?}"))
        .clone();

    let error = crate::support::mcp_process::tool_error(&response);
    assert_eq!(error["diagnostic"]["code"], "E063", "{response}");
    let hint = error["diagnostic"]["suggestion"]
        .as_str()
        .unwrap_or_default();
    assert!(
        hint.contains("\"registries\"") && hint.contains("specforge.json"),
        "the hint must say how to configure a registry: {response}"
    );
    assert_eq!(spy.hits(), 0, "add_extension reached the network");
}

// ===============================================================
// The registry configuration's own diagnostics are shown
// ===============================================================
//
// Reading `registries` reports E067 (an unreadable entry), W140 (a
// duplicate alias) and I003 (no default registry). The registry commands
// show them on stderr; an entry that can't be read is skipped, and when
// none can be read the command fails with E067 instead of E063.

const UNREACHABLE: &str = "http://registry.invalid";

/// A project whose specforge.json has `registries` set to `registries`.
fn project_with_registries(registries: serde_json::Value) -> TestProject {
    TestProject::new().config(|c| {
        c["name"] = "demo".into();
        c["registries"] = registries;
    })
}

/// `specforge <args> --path <dir>` under a spy; returns its output.
fn run_spied(args: &[&str], dir: &TestProject) -> std::process::Output {
    NetSpy::start().on(dir).command(args).output().unwrap()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[specforge_test(
    behavior = "configure_registries",
    verify = "duplicate alias produces warning"
)]
fn search_shows_a_duplicate_registry_alias() {
    let dir = project_with_registries(serde_json::json!([
        {"alias": "main", "url": UNREACHABLE, "default_registry": true},
        {"alias": "main", "url": UNREACHABLE},
    ]));
    let stderr = stderr_of(&run_spied(&["search", "widget"], &dir));
    assert!(
        stderr.contains("warning[W140]: Duplicate registry alias \"main\""),
        "{stderr}"
    );
}

#[test]
fn add_from_a_registry_shows_a_duplicate_registry_alias() {
    let dir = project_with_registries(serde_json::json!([
        {"alias": "main", "url": UNREACHABLE, "default_registry": true},
        {"alias": "main", "url": UNREACHABLE},
    ]));
    let stderr = stderr_of(&run_spied(&["add", "@acme/widget@1.0.0"], &dir));
    assert!(stderr.contains("warning[W140]"), "{stderr}");
}

#[test]
fn adding_a_builtin_does_not_read_the_registries() {
    let dir = project_with_registries(serde_json::json!([
        {"alias": "main", "url": UNREACHABLE, "default_registry": true},
        {"alias": "main", "url": UNREACHABLE},
    ]));
    let output = run_spied(&["add", "@specforge/software"], &dir);
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(
        !stderr_of(&output).contains("W140"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn search_shows_that_no_registry_is_the_default() {
    let dir = project_with_registries(serde_json::json!([
        {"alias": "main", "url": UNREACHABLE},
    ]));
    let stderr = stderr_of(&run_spied(&["search", "widget"], &dir));
    assert!(stderr.contains("info[I003]"), "{stderr}");
}

#[specforge_test(
    behavior = "configure_registries",
    verify = "a registries entry that can't be read refuses every registry operation with E067 before any request"
)]
fn login_refuses_a_configuration_with_an_unreadable_entry_before_any_request() {
    let spy = NetSpy::start();
    let dir = project_with_registries(serde_json::json!([
        {"alias": "main", "url": UNREACHABLE, "default_registry": true},
        {"alias": "broken"},
    ]));
    let output = spy
        .on(&dir)
        .command(&["login", "--token", "t", "--format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(json["code"], "E067", "{json}");
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("index 1"),
        "{json}"
    );
    assert_eq!(spy.hits(), 0, "login reached the network");
}

#[test]
fn search_with_no_readable_registry_entry_fails_with_e067() {
    let dir = project_with_registries(serde_json::json!([{"alias": "broken"}]));
    let output = run_spied(&["search", "widget", "--format", "json"], &dir);
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(json["code"], "E067", "{json}");
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("index 0"),
        "{json}"
    );
}

#[specforge_test(
    behavior = "configure_registries",
    verify = "builtins and local .wasm files install with no registry configured"
)]
fn builtins_and_local_wasm_install_without_registry() {
    let spy = NetSpy::start();
    let dir = project_without_registry();
    let wasm = dir.root().join("local-ext.wasm");
    std::fs::write(&wasm, greet_wasm()).unwrap();

    for specifier in ["@specforge/product", wasm.to_str().unwrap()] {
        let output = spy
            .on(&dir)
            .command(&["add"])
            .arg(specifier)
            .arg("--format")
            .arg("json")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "add {specifier}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let config = std::fs::read_to_string(dir.root().join("specforge.json")).unwrap();
    assert!(config.contains("@specforge/product"), "{config}");
    let lock = std::fs::read_to_string(dir.root().join("specforge.lock")).unwrap();
    assert!(lock.contains("local:local-ext.wasm"), "{lock}");
    assert_eq!(spy.hits(), 0, "offline installs reached the network");
}

// ---------------------------------------------------------------
// Registry installs against a local fake registry
// ---------------------------------------------------------------

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add extension without version resolves to latest compatible version"
)]
fn add_without_a_version_installs_the_latest() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![
        Package::new("@sdk/greet", "0.0.1", greet_wasm()),
        Package::new("@sdk/greet", "0.1.0", greet_wasm()),
    ]);
    let dir = project_on(&registry);

    let output = Cli::on(&dir)
        .command(&["add", "@sdk/greet", "--allow-unsigned", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["added"][0]["extension"], "@sdk/greet", "{json}");
    assert_eq!(json["added"][0]["version"], "0.1.0", "{json}");
    let lock = std::fs::read_to_string(dir.root().join("specforge.lock")).unwrap();
    assert!(lock.contains("\"0.1.0\""), "{lock}");
}

// ---------------------------------------------------------------
// Publisher trust through `specforge add` (docs/registry-trust.md)
// ---------------------------------------------------------------

/// `specforge add @sdk/greet@0.1.0 --format json` in `dir` (its HOME holds the known-keys store), plus `extra`
/// flags.
fn add_greet(dir: &TestProject, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["add", "@sdk/greet@0.1.0", "--format", "json"];
    args.extend(extra);
    Cli::on(dir).command(&args).output().unwrap()
}

/// The add failed with `code`, and installed nothing.
fn assert_refused(output: &std::process::Output, dir: &TestProject, code: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "expected failure: {stdout}");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(json["code"], code, "{json}");
    assert!(
        !dir.root().join("specforge.lock").exists(),
        "a refused package is not locked"
    );
}

pub(crate) fn known_keys(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".specforge").join("known-keys.json")
}

#[specforge_test(
    invariant = "publisher_trust",
    verify = "specforge add pins the publisher key and records it in specforge.lock"
)]
fn add_installs_a_signed_package_and_pins_its_key() {
    use crate::support::registry::{Package, serve};
    let key = specforge_registry_client::SigningKey::generate();
    let registry = serve(vec![
        Package::new("@sdk/greet", "0.1.0", greet_wasm()).signed_by(&key),
    ]);
    let dir = project_on(&registry);

    let output = add_greet(&dir, &[]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let known = specforge_registry_client::load_known_keys_at(&known_keys(dir.home())).unwrap();
    assert_eq!(known.pin_for("@sdk/greet"), Some(key.key_id().as_str()));
    let lock = std::fs::read_to_string(dir.root().join("specforge.lock")).unwrap();
    assert!(
        lock.contains(&key.key_id()),
        "the lock records the key: {lock}"
    );
}

#[specforge_test(
    invariant = "publisher_trust",
    verify = "specforge add refuses a package signed by another key than the pinned one"
)]
fn add_refuses_a_package_signed_by_another_key_than_the_pinned_one() {
    use crate::support::registry::{Package, serve};
    let pinned = specforge_registry_client::SigningKey::generate();
    let other = specforge_registry_client::SigningKey::generate();
    let registry = serve(vec![
        Package::new("@sdk/greet", "0.1.0", greet_wasm()).signed_by(&other),
    ]);
    let dir = project_on(&registry);
    let mut known = specforge_registry_client::KnownKeys::default();
    known.pin("@sdk/greet", &pinned.key_id());
    specforge_registry_client::save_known_keys_at(&known_keys(dir.home()), &known).unwrap();

    let output = add_greet(&dir, &["--allow-unsigned"]);

    assert_refused(&output, &dir, "R-TRUST-003");
    let known = specforge_registry_client::load_known_keys_at(&known_keys(dir.home())).unwrap();
    assert_eq!(known.pin_for("@sdk/greet"), Some(pinned.key_id().as_str()));
}

#[specforge_test(
    invariant = "publisher_trust",
    verify = "specforge add refuses an unsigned package without --allow-unsigned"
)]
fn add_refuses_an_unsigned_package_without_allow_unsigned() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_on(&registry);

    let output = add_greet(&dir, &[]);

    assert_refused(&output, &dir, "R-TRUST-001");
}

// §3 R3 (plan 05): nothing asked a registry, so its configuration is not shown.
#[specforge_test(
    behavior = "configure_registries",
    verify = "an operation shows the registry configuration's diagnostics once it has asked a registry"
)]
fn adding_an_installed_registry_package_shows_no_registry_configuration() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    // The served registry is the default; a second entry repeats an alias (W140).
    let dir = TestProject::new().without_config();
    let config = serde_json::json!({
        "name": "p",
        "version": "0.1.0",
        "extensions": ["@specforge/software"],
        "registries": [
            {"alias": "local", "url": registry.root().as_str(), "default_registry": true},
            {"alias": "local", "url": "http://registry.invalid"},
        ],
    });
    dir.write("specforge.json", config.to_string());
    let add = || {
        Cli::on(&dir)
            .command(&["add", "@sdk/greet@0.1.0", "--allow-unsigned"])
            .output()
            .unwrap()
    };

    let first = add();
    assert!(first.status.success(), "{}", stderr_of(&first));
    let hits = registry.requests().len();

    let again = add();
    assert!(again.status.success(), "{}", stderr_of(&again));
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("already installed"),
        "{}",
        String::from_utf8_lossy(&again.stdout)
    );
    assert_eq!(registry.requests().len(), hits, "nothing was asked");
    assert!(
        !stderr_of(&again).contains("W140"),
        "nothing asked a registry: {}",
        stderr_of(&again)
    );
}

// ---------------------------------------------------------------
// Guard: no source names specforge.dev
// ---------------------------------------------------------------

/// Files allowed to name `specforge.dev`, with the text every occurrence
/// there must contain. These are the schema URLs, which plan step 06·C6
/// moves to the canonical repository; that step deletes these rows (the
/// test fails on a row that matches nothing).
const SPECFORGE_DEV_ALLOWED: &[(&str, &str)] = &[
    (
        "crates/pipeline/emitter/src/schema.rs",
        "\"https://specforge.dev/schema/graph-protocol-v{}.json\"",
    ),
    (
        "crates/app/ops/src/authoring/init.rs",
        "\"$schema\": \"https://specforge.dev/schema/specforge.json\"",
    ),
];

/// Replace `#[cfg(test)]` / `#[test]` items (brace-matched) with blank
/// lines, keeping line numbers.
fn strip_test_items(src: &str) -> Vec<&str> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("#[cfg(test)]") || trimmed == "#[test]" {
            let mut j = i + 1;
            let mut depth: i64 = 0;
            let mut started = false;
            while j < lines.len() {
                for ch in lines[j].chars() {
                    match ch {
                        '{' => {
                            depth += 1;
                            started = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                if (started && depth <= 0) || (!started && lines[j].trim_end().ends_with(';')) {
                    break;
                }
                j += 1;
            }
            let end = j.min(lines.len() - 1);
            out.extend(std::iter::repeat_n("", end - i + 1));
            i = end + 1;
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }
    out
}

/// Every file under `dir`, skipping test directories and build output.
fn non_test_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    const SKIP: &[&str] = &["tests", "target", "node_modules", ".git"];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !SKIP.contains(&name.as_str()) {
                non_test_files(&path, files);
            }
        } else {
            files.push(path);
        }
    }
}

#[specforge_test(
    behavior = "configure_registries",
    verify = "no registry URL on the specforge.dev domain is compiled into non-test source"
)]
fn no_source_names_specforge_dev() {
    let root = specforge_repo_root::repo_root();
    let mut files = Vec::new();
    for top in ["crates", "extensions"] {
        non_test_files(&root.join(top), &mut files);
    }
    assert!(files.len() > 100, "scanner found {} files", files.len());

    let mut problems = Vec::new();
    let mut allowed_used = std::collections::BTreeSet::new();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        // Binary files (the wasm blobs) are built from the sources read here.
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file_name = rel.rsplit('/').next().unwrap_or_default();
        if file_name == "tests.rs" || file_name.ends_with("_tests.rs") {
            continue;
        }
        let lines = if rel.ends_with(".rs") {
            strip_test_items(&src)
        } else {
            src.lines().collect()
        };
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("specforge.dev") {
                continue;
            }
            match SPECFORGE_DEV_ALLOWED
                .iter()
                .find(|(file, text)| *file == rel && line.contains(text))
            {
                Some(row) => {
                    allowed_used.insert(*row);
                }
                None => problems.push(format!("{rel}:{}: {}", index + 1, line.trim())),
            }
        }
    }
    for row in SPECFORGE_DEV_ALLOWED {
        if !allowed_used.contains(row) {
            problems.push(format!(
                "SPECFORGE_DEV_ALLOWED row {row:?} matches nothing: delete it"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "SpecForge does not own specforge.dev (ADR 0004 N1); remove these:\n  {}",
        problems.join("\n  ")
    );
}

#[specforge_test(
    behavior = "logout_registry",
    verify = "logout without --registry forgets the default registry's credential"
)]
fn logout_without_a_registry_forgets_the_default_registrys_credential() {
    let spy = NetSpy::start();
    let (default, other) = ("default", "other");
    let default_root = format!("{UNREACHABLE}/default");
    let other_root = format!("{UNREACHABLE}/other");
    let dir = TestProject::new().raw_config(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": [],
            "registries": [
                { "alias": default, "url": default_root, "default_registry": true },
                { "alias": other, "url": other_root, "scope_filter": "@other" },
            ],
        })
        .to_string(),
    );
    let store = dir.home().join(".specforge");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(
        store.join("credentials.json"),
        serde_json::json!({
            "registries": { default_root.clone(): { "token": "t1" }, other_root.clone(): { "token": "t2" } }
        })
        .to_string(),
    )
    .unwrap();

    let output = spy
        .on(&dir)
        .command(&["logout", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["registry"], default, "{json}");
    assert_eq!(json["status"], "removed", "{json}");
    let kept: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.join("credentials.json")).unwrap()).unwrap();
    let roots: Vec<&String> = kept["registries"].as_object().unwrap().keys().collect();
    assert_eq!(roots, [&other_root]);
    assert_eq!(spy.hits(), 0);
}

#[test]
fn login_with_an_unknown_registry_is_refused_before_any_request() {
    let spy = NetSpy::start();
    let dir = TestProject::new().raw_config(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": [],
            "registries": [{ "alias": "main", "url": UNREACHABLE, "default_registry": true }],
        })
        .to_string(),
    );

    let output = spy
        .on(&dir)
        .command(&[
            "login",
            "--registry",
            "typo",
            "--token",
            "t",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "E063", "{json}");
    let message = json["error"].as_str().unwrap();
    assert!(
        message.contains("typo") && message.contains("main"),
        "{json}"
    );
    assert_eq!(spy.hits(), 0, "login reached the network");
}

// ---------------------------------------------------------------
// Pins of plan 16 (T0): what the registry commands do today
// ---------------------------------------------------------------

#[specforge_test(
    behavior = "search_registry",
    verify = "each failed registry is reported once, and search fails when every registry failed"
)]
fn a_failed_registry_is_reported_once_and_search_fails_when_all_did() {
    let dir = project_with_registries(serde_json::json!([
        {"alias": "down", "url": "http://127.0.0.1:9", "default_registry": true}
    ]));

    let output = Cli::on(&dir).command(&["search", "x"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_of(&output);
    assert_eq!(
        stderr.matches("search failed on registry 'down'").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("R005"), "{stderr}");
}

#[test]
fn search_names_the_registry_of_each_hit() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_on(&registry);

    let output = Cli::on(&dir)
        .command(&["search", "greet", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"][0]["registry"], "local", "{json}");
    assert!(json.get("contributes").is_none(), "{json}");
    assert_eq!(json["diagnostics"], serde_json::json!([]), "{json}");
}

#[test]
fn an_unknown_category_is_refused_before_any_request() {
    use crate::support::registry::serve;
    let registry = serve(vec![]);
    let dir = project_on(&registry);

    let output = Cli::on(&dir)
        .command(&["search", "x", "--contributes", "widgets"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("surfaces"), "{stderr}");
    assert!(registry.requests().is_empty());
}

#[specforge_test(
    behavior = "verify_publisher_signature",
    verify = "an unsigned package accepted with --allow-unsigned is reported as W155"
)]
fn an_unsigned_add_reports_w155_in_both_formats() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);

    let add = |format: &str| {
        let dir = project_on(&registry);
        let output = Cli::on(&dir)
            .command(&[
                "add",
                "@sdk/greet@0.1.0",
                "--allow-unsigned",
                "--format",
                format,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        output
    };

    let human = add("human");
    assert!(
        stderr_of(&human).contains("warning[W155]"),
        "{}",
        stderr_of(&human)
    );
    assert!(
        stderr_of(&human).contains("was accepted because unsigned packages were allowed"),
        "{}",
        stderr_of(&human)
    );
    let json = add("json");
    assert!(
        stderr_of(&json).contains("warning[W155]"),
        "{}",
        stderr_of(&json)
    );
    let doc: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(doc["added"][0]["publisher"], "unsigned", "{doc}");
}

// ---------------------------------------------------------------
// login keeps a secret or a reference (plan 16, T3)
// ---------------------------------------------------------------

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "valid credentials stored as RegistryCredential reference"
)]
fn login_with_a_token_variable_keeps_the_reference() {
    use crate::support::registry::serve;
    let registry = serve(vec![]);
    let dir = project_on(&registry);

    let output = Cli::on(&dir)
        .command(&[
            "login",
            "--registry",
            "local",
            "--token-env",
            "P16_LOGIN_TOKEN",
            "--format",
            "json",
        ])
        .env("P16_LOGIN_TOKEN", registry.token())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["source"], "env", "{json}");
    let stored = std::fs::read_to_string(dir.home().join(".specforge/credentials.json")).unwrap();
    assert!(
        stored.contains("\"token_env\": \"P16_LOGIN_TOKEN\""),
        "{stored}"
    );
    assert!(!stored.contains(registry.token()), "{stored}");
    assert!(!dir.home().join(".specforge/secrets").exists());
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "login with a token file keeps the file's path, not its content"
)]
fn login_with_a_token_file_keeps_the_path() {
    use crate::support::registry::serve;
    let registry = serve(vec![]);
    let dir = project_on(&registry);
    let file = dir.home().join("token.txt");
    std::fs::write(&file, format!("{}\n", registry.token())).unwrap();

    let output = Cli::on(&dir)
        .command(&[
            "login",
            "--registry",
            "local",
            "--token-file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["source"], "token_file", "{json}");
    let stored = std::fs::read_to_string(dir.home().join(".specforge/credentials.json")).unwrap();
    assert!(stored.contains("token_file"), "{stored}");
    assert!(!stored.contains(registry.token()), "{stored}");
}

#[specforge_test(
    behavior = "validate_registry_credentials",
    verify = "login takes exactly one token source"
)]
fn login_with_two_token_sources_is_refused() {
    use crate::support::registry::serve;
    let registry = serve(vec![]);
    let dir = project_on(&registry);

    let output = Cli::on(&dir)
        .command(&[
            "login",
            "--registry",
            "local",
            "--token",
            "x",
            "--token-env",
            "Y",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "R-LOGIN-001", "{json}");
    assert!(registry.requests().is_empty(), "{:?}", registry.requests());
}

#[specforge_test(
    behavior = "support_private_registries",
    verify = "a registry that requires a token to read is read with the stored credential"
)]
fn add_from_a_private_registry_with_a_stored_credential() {
    use crate::support::registry::Package;
    let registry = specforge_registry_server::testing::LocalRegistry::start_private();
    let package = Package::new("@sdk/greet", "0.1.0", greet_wasm());
    registry.store(
        &specforge_registry_client::testing::package(
            &package.name,
            &package.version,
            &package.wasm,
            &package.manifest(),
            None,
        ),
        &package.wasm,
    );
    let dir = project_on(&registry);

    // With the credential the user keeps for the registry: installed.
    std::fs::create_dir_all(dir.home().join(".specforge")).unwrap();
    std::fs::write(
        dir.home().join(".specforge/credentials.json"),
        serde_json::json!({"registries": {registry.root().as_str(): {"token": registry.token()}}})
            .to_string(),
    )
    .unwrap();
    let output = add_greet(&dir, &["--allow-unsigned"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );

    // Without it: refused as unauthenticated, nothing installed.
    let dir = project_on(&registry);
    let output = add_greet(&dir, &["--allow-unsigned"]);
    assert_refused(&output, &dir, "R001");
}

// ---------------------------------------------------------------
// Plan 09: trust pins. These tests pin today's behaviour (a refused install
// pins its publisher key); the ticket that fixes it flips them.
// ---------------------------------------------------------------

/// What `Trusting` holds in the registry and the project.
struct TrustingSetup {
    /// `home` pins `@sdk/greet` to k1.
    pin_k1: bool,
    /// The registry serves greet 0.1.0 signed by k2 instead of k1.
    greet_signed_by_k2: bool,
    /// The project locks `@x/a` (which needs greet `^0.1`).
    lock_app: bool,
    /// More packages the registry serves.
    more: Vec<crate::support::registry::Package>,
}

/// The registry serves `@sdk/greet` 0.1.0 (the greet binary) signed by `k1`, 0.2.0 (bytes that are no
/// extension) signed by `k2`, and `@x/a` 1.0.0 (peer `@sdk/greet ^0.1`) signed by `k1`. The project locks greet
/// 0.1.0 with no module (adding it installs it again) and `@x/a` 1.0.0 with its module, both with key `k1`.
struct Trusting {
    _registry: specforge_registry_server::testing::LocalRegistry,
    dir: TestProject,
    k1: specforge_registry_client::SigningKey,
    k2: specforge_registry_client::SigningKey,
}

fn trusting(setup: TrustingSetup) -> Trusting {
    use crate::support::registry::{Package, serve};
    let k1 = specforge_registry_client::SigningKey::generate();
    let k2 = specforge_registry_client::SigningKey::generate();
    let greet_signer = if setup.greet_signed_by_k2 { &k2 } else { &k1 };
    let mut packages = vec![
        Package::new("@sdk/greet", "0.1.0", greet_wasm()).signed_by(greet_signer),
        Package::new("@sdk/greet", "0.2.0", b"\0asm greet 0.2.0".to_vec()).signed_by(&k2),
        Package::new("@x/a", "1.0.0", b"\0asm x/a".to_vec())
            .with_peer("@sdk/greet", "^0.1")
            .signed_by(&k1),
    ];
    packages.extend(setup.more);
    let registry = serve(packages);
    let mut extensions = vec!["@specforge/software".to_string(), "@sdk/greet".to_string()];
    let mut entries = vec![specforge_installed::LockFileEntry {
        name: "@sdk/greet".parse().unwrap(),
        version: "0.1.0".into(),
        source: specforge_installed::LockSource::Registry,
        wasm_hash: specforge_installed::hex_sha256(&greet_wasm()),
        key_id: Some(k1.key_id()),
        peer_dependencies: Vec::new(),
    }];
    if setup.lock_app {
        extensions.push("@x/a".to_string());
        entries.push(specforge_installed::LockFileEntry {
            name: "@x/a".parse().unwrap(),
            version: "1.0.0".into(),
            source: specforge_installed::LockSource::Registry,
            wasm_hash: specforge_installed::hex_sha256(b"\0asm x/a"),
            key_id: Some(k1.key_id()),
            peer_dependencies: vec![specforge_protocol_types::PeerDependency {
                name: "@sdk/greet".into(),
                version: "^0.1".into(),
                optional: false,
            }],
        });
    }
    let dir = project_on(&registry)
        .file(".specforge/extensions/@x/a/extension.wasm", b"\0asm x/a")
        .enabling(&extensions.iter().map(String::as_str).collect::<Vec<_>>());
    specforge_installed::write_lock_file(
        &specforge_installed::LockFile {
            lockfile_version: 1,
            entries,
        },
        &dir.root().join("specforge.lock"),
    )
    .unwrap();
    if setup.pin_k1 {
        let mut known = specforge_registry_client::KnownKeys::default();
        known.pin("@sdk/greet", &k1.key_id());
        std::fs::create_dir_all(dir.home().join(".specforge")).unwrap();
        specforge_registry_client::save_known_keys_at(&known_keys(dir.home()), &known).unwrap();
    }
    Trusting {
        _registry: registry,
        dir,
        k1,
        k2,
    }
}

impl Trusting {
    fn run(&self, args: &[&str]) -> std::process::Output {
        Cli::on(&self.dir).command(args).output().unwrap()
    }

    fn pinned(&self, name: &str) -> Option<String> {
        specforge_registry_client::load_known_keys_at(&known_keys(self.dir.home()))
            .unwrap()
            .pin_for(name)
            .map(str::to_string)
    }

    fn locked_key(&self, name: &str) -> Option<String> {
        specforge_installed::read_lock_file(&self.dir.root().join("specforge.lock"))
            .unwrap()
            .entries
            .into_iter()
            .find(|e| e.name.as_str() == name)
            .and_then(|e| e.key_id)
    }
}

fn json_of(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"))
}

fn setup(pin_k1: bool) -> TrustingSetup {
    TrustingSetup {
        pin_k1,
        greet_signed_by_k2: false,
        lock_app: true,
        more: Vec::new(),
    }
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a refused install pins no key, so the locked version installs again"
)]
fn a_refused_install_pins_no_key_so_the_locked_version_installs_again() {
    let t = trusting(setup(true));

    let refused = t.run(&["add", "@sdk/greet@0.2.0", "--yes", "--format", "json"]);

    assert!(!refused.status.success());
    assert_eq!(json_of(&refused)["code"], "E028");
    assert!(
        !stderr_of(&refused).contains("W156"),
        "{}",
        stderr_of(&refused)
    );
    assert_eq!(t.pinned("@sdk/greet"), Some(t.k1.key_id()));
    assert_eq!(t.locked_key("@sdk/greet"), Some(t.k1.key_id()));
    let again = t.run(&["add", "@sdk/greet@0.1.0", "--format", "json"]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stdout)
    );
    assert_eq!(
        json_of(&again)["added"][0]["publisher"],
        "pinned",
        "{again:?}"
    );
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a refused update pins no key"
)]
fn a_refused_update_pins_no_key() {
    let t = trusting(setup(true));

    let output = t.run(&["update", "--major", "--yes", "--format", "json"]);

    assert!(!output.status.success());
    let json = json_of(&output);
    assert_eq!(json["ok"], false, "{json}");
    let greet = json["extensions"]
        .as_array()
        .or_else(|| json["results"].as_array())
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["extension"] == "@sdk/greet" || row["name"] == "@sdk/greet")
        })
        .unwrap_or_else(|| panic!("no greet row: {json}"));
    assert_eq!(greet["status"], "failed", "{json}");
    assert_eq!(greet["code"], "E028", "{json}");
    assert!(
        !stderr_of(&output).contains("W156"),
        "{}",
        stderr_of(&output)
    );
    assert_eq!(t.pinned("@sdk/greet"), Some(t.k1.key_id()));
}

#[test]
fn a_refused_install_pins_no_first_use_key() {
    let t = trusting(setup(false));

    let output = t.run(&["add", "@sdk/greet@0.2.0", "--format", "json"]);

    assert!(!output.status.success());
    assert_eq!(json_of(&output)["code"], "R-TRUST-003");
    assert!(
        !known_keys(t.dir.home()).exists(),
        "a refused install creates no pin store"
    );
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "with no pin of the user's, the key specforge.lock records is the pin"
)]
fn with_no_pin_the_key_the_lock_records_is_the_pin() {
    let t = trusting(TrustingSetup {
        greet_signed_by_k2: true,
        ..setup(false)
    });

    let lock_before = std::fs::read(t.dir.root().join("specforge.lock")).unwrap();

    let output = t.run(&["add", "@sdk/greet@0.1.0", "--format", "json"]);

    assert!(!output.status.success());
    let json = json_of(&output);
    assert_eq!(json["code"], "R-TRUST-003", "{json}");
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains(&format!("specforge.lock records '{}'", t.k1.key_id())),
        "{json}"
    );
    assert!(!known_keys(t.dir.home()).exists());
    assert_eq!(
        std::fs::read(t.dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
    // The user can accept the key the registry serves now.
    let accepted = t.run(&["add", "@sdk/greet@0.1.0", "--yes", "--format", "json"]);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stdout)
    );
    assert_eq!(t.locked_key("@sdk/greet"), Some(t.k2.key_id()));
    assert_eq!(t.pinned("@sdk/greet"), Some(t.k2.key_id()));
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "an install refused when its binary loads pins no key"
)]
fn an_install_refused_at_the_load_pins_no_key() {
    let t = trusting(TrustingSetup {
        lock_app: false,
        ..setup(true)
    });

    let output = t.run(&["add", "@sdk/greet@0.2.0", "--yes", "--format", "json"]);

    assert!(!output.status.success());
    assert_eq!(json_of(&output)["code"], "E028");
    assert!(
        !stderr_of(&output).contains("W156"),
        "{}",
        stderr_of(&output)
    );
    assert_eq!(t.pinned("@sdk/greet"), Some(t.k1.key_id()));
    assert_eq!(t.locked_key("@sdk/greet"), Some(t.k1.key_id()));
}

#[specforge_test(
    behavior = "pin_publisher_key",
    verify = "a refused add of several specifiers pins no key"
)]
fn a_refused_add_of_several_specifiers_pins_no_key() {
    use crate::support::registry::Package;
    let t = trusting(TrustingSetup {
        more: vec![Package::new("@x/plain", "1.0.0", b"\0asm plain".to_vec())],
        ..setup(true)
    });
    let lock_before = std::fs::read(t.dir.root().join("specforge.lock")).unwrap();
    let config_before = std::fs::read(t.dir.root().join("specforge.json")).unwrap();

    let output = t.run(&[
        "add",
        "@sdk/greet@0.2.0",
        "@x/plain@1.0.0",
        "--yes",
        "--format",
        "json",
    ]);

    assert!(!output.status.success());
    let json = json_of(&output);
    assert_eq!(json["code"], "R-TRUST-001", "{json}");
    assert!(
        json["error"].as_str().unwrap().contains("@x/plain@1.0.0"),
        "{json}"
    );
    assert_eq!(
        std::fs::read(t.dir.root().join("specforge.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        std::fs::read(t.dir.root().join("specforge.json")).unwrap(),
        config_before
    );
    assert!(
        !stderr_of(&output).contains("W156"),
        "{}",
        stderr_of(&output)
    );
    assert_eq!(t.pinned("@sdk/greet"), Some(t.k1.key_id()));
}

/// `specforge <args> --format json --path <dir>`; its exit status and JSON.
fn json_run(args: &[&str], dir: &TestProject) -> (bool, serde_json::Value, std::process::Output) {
    let mut argv = args.to_vec();
    argv.extend(["--format", "json"]);
    let output = Cli::on(dir).command(&argv).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let json = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    (output.status.success(), json, output)
}

#[specforge_test(
    behavior = "search_registry",
    verify = "queries all configured registries"
)]
fn a_registry_named_by_its_root_is_searched() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let dir = project_with_registries(
        serde_json::json!([{"alias": "local", "url": registry.root().as_str(), "default_registry": true}]),
    );

    let (ok, json, _) = json_run(&["search", "greet"], &dir);

    assert!(ok, "{json}");
    assert!(json.to_string().contains("@sdk/greet"), "{json}");
    assert!(json.to_string().contains("0.1.0"), "{json}");
}

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "every registry call reads an answer's status as one error"
)]
fn login_at_an_address_no_registry_serves_is_r009() {
    use crate::support::registry::{Package, serve};
    let registry = serve(vec![Package::new("@sdk/greet", "0.1.0", greet_wasm())]);
    let root = registry.root().as_str();
    let dir = project_with_registries(serde_json::json!([
        {"alias": "local", "url": format!("{root}/elsewhere"), "default_registry": true}
    ]));

    let (ok, json, _) = json_run(&["login", "--token", "t"], &dir);

    assert!(!ok, "{json}");
    assert_eq!(json["code"], "R009", "{json}");
}

#[specforge_test(
    behavior = "configure_registries",
    verify = "a registry url is the registry's root, and one ending in the protocol's version prefix is refused with E067 naming the root"
)]
fn a_registry_url_with_the_version_prefix_is_refused_before_any_request() {
    let spy = NetSpy::start();
    let dir = project_with_registries(serde_json::json!([
        {"alias": "main", "url": "http://registry.invalid/v1", "default_registry": true}
    ]));
    let output = spy
        .on(&dir)
        .command(&["search", "greet", "--format", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(json["code"], "E067", "{json}");
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("name the registry's root: 'http://registry.invalid'"),
        "{json}"
    );
    assert_eq!(spy.hits(), 0, "search reached the network");
}
