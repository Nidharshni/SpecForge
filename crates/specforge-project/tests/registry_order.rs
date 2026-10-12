//! Characterization of the registry diagnostics' order (architecture plan
//! 05, step R0).
//!
//! `CompiledProject::compile` loads the extensions, populates the registries,
//! parses the rules and registers the surfaces in a fixed order, and its
//! diagnostics come out in that order: populate (E026, W018), then rule
//! parsing (W112), the surface conflicts (E039) with them, before the
//! graph's own (here E002).

use specforge_common::Diagnostic;
use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta, PeerDependency};
use specforge_wasm::testing::InProcessRuntime;
use std::sync::Arc;

/// Two extensions that collide on everything the registry build checks:
/// the same entity kind (E026), the same edge label (W018) and the same CLI
/// command (E039); each also declares a rule whose check kind doesn't
/// exist (W112).
fn colliding_extensions() -> InProcessRuntime {
    colliding_extensions_with(false, false)
}

/// `colliding_extensions`, and optionally: `@test/alpha` requires a peer that
/// is not loaded (E027), and declares a second rule whose custom function its
/// extension does not export (the probe's W112).
fn colliding_extensions_with(peer: bool, custom: bool) -> InProcessRuntime {
    ["@test/alpha", "@test/beta"]
        .into_iter()
        .fold(InProcessRuntime::new(), |runtime, ext| {
            runtime.with(move || {
                let mut c = ContributionsBuilder::new(ExtensionMeta::new(ext, "1.0.0"));
                if peer && ext == "@test/alpha" {
                    c.meta.peer_dependencies = vec![PeerDependency {
                        name: "@test/missing".into(),
                        version: "^1".into(),
                        optional: false,
                    }];
                }
                let mut rules = vec![serde_json::json!({
                    "code": "W900",
                    "severity": "warning",
                    "message_template": "never fires",
                    "check": format!("no_such_check_{}", &ext[6..]),
                    "target_kind": "gadget"
                })];
                if custom && ext == "@test/alpha" {
                    rules.push(serde_json::json!({
                        "code": "W901",
                        "severity": "warning",
                        "message_template": "never answers",
                        "check": "custom",
                        "wasm_function": "missing_validator",
                        "target_kind": "gadget"
                    }));
                }
                // Declared as given (`raw_category`): a builder would refuse
                // a rule whose check does not exist.
                c.raw_category(
                    specforge_extension_sdk::DeclaredCategory::Entities,
                    serde_json::json!([{"name": "gadget", "description": format!("gadget from {ext}")}]),
                );
                c.raw_category(
                    specforge_extension_sdk::DeclaredCategory::Edges,
                    serde_json::json!([{"label": "GadgetUses", "source_kind": "gadget", "target_kind": "gadget"}]),
                );
                c.raw_category(
                    specforge_extension_sdk::DeclaredCategory::ValidationRules,
                    serde_json::Value::Array(rules),
                );
                c.raw_category(
                    specforge_extension_sdk::DeclaredCategory::Surfaces,
                    serde_json::json!([{"commands": [{
                        "id": "hello",
                        "title": "Hello",
                        "description": format!("hello from {ext}"),
                        "export": "run_hello"
                    }]}]),
                );
                c
            })
        })
}

fn project() -> tempfile::TempDir {
    project_with(false)
}

/// The project, optionally with a provider whose extension is not installed (W118).
fn project_with(providers: bool) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let mut config = serde_json::json!({
        "name": "order",
        "version": "0.1.0",
        "extensions": ["@test/alpha", "@test/beta"]
    });
    if providers {
        config["providers"] =
            serde_json::json!([{"scheme": "gh", "alias": "work", "extension": "@acme/github"}]);
    }
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    specforge_installed::testing::install_configured(dir.path(), &specforge_project::builtins());
    // A duplicate ID: a graph diagnostic, between the registry ones and E039.
    std::fs::write(
        dir.path().join("main.spec"),
        "gadget widget \"Widget\" {\n}\ngadget widget \"Again\" {\n}\n",
    )
    .unwrap();
    dir
}

fn render(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(specforge_common::format_diagnostic)
        .collect()
}

// A characterization, not linked: `diagnostic_determinism` is not proven
// while `check`'s W113 names an import cycle in HashMap order (see the
// parity harness in specforge-cli).
#[test]
fn registry_diagnostics_keep_their_order() {
    let dir = project();
    let runtime = Arc::new(colliding_extensions());

    let expected = [
        "<E026>: error[E026]: entity kind 'gadget' registered by '@test/beta' conflicts with '@test/alpha' (first registration wins)",
        "<W018>: warning[W018]: edge type 'GadgetUses' from '@test/beta' duplicates 'GadgetUses' from '@test/alpha' (first wins)",
        "<W112>: warning[W112]: extension '@test/alpha': unrecognized validation pattern kind 'no_such_check_alpha'",
        "<W112>: warning[W112]: extension '@test/beta': unrecognized validation pattern kind 'no_such_check_beta'",
        // Both extensions declare rule W900 (05·R4 wired W023, after the
        // rule-parse diagnostics).
        "<W023>: warning[W023]: validation rule code 'W900' from '@test/beta' duplicates code from '@test/alpha'",
        "<E039>: error[E039]: duplicate surface command ID 'hello': extension '@test/beta' conflicts with '@test/alpha'",
        "main.spec:3:1: error[E002]: duplicate entity ID 'widget' (first declared at main.spec:1:1)\n  help: rename one of the entities to avoid the collision",
    ];

    // Compiled again and again, with fresh hash maps each time: the order
    // must not depend on them.
    for _ in 0..20 {
        let ctx = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Some(runtime.clone()),
        );
        let diagnostics = ctx.diagnostics();
        assert_eq!(render(&diagnostics), expected);
    }
}

// This project has no import cycle, so `diagnostic_determinism` is not at
// risk from W113.
#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "the environment reports its own diagnostics, then the registry build's, before any source"
)]
fn the_environment_reports_before_the_sources() {
    let dir = project_with(true);
    let runtime = Arc::new(colliding_extensions_with(true, true));

    for _ in 0..20 {
        let ctx = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Some(runtime.clone()),
        );
        let diagnostics = ctx.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        // The environment's own first (provider registration), then the
        // build's (the declarations' E027, the registries', the probe's, the
        // surface conflict), then the graph's and the checks'.
        assert_eq!(
            codes,
            [
                "W118", "E027", "E026", "W018", "W112", "W112", "W023", "W112", "E039", "E002",
                "W148"
            ]
        );
        let probe = diagnostics.iter().rfind(|d| d.code == "W112").unwrap();
        assert!(probe.message.contains("could not be resolved"), "{probe:?}");
    }
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "the environment says what part of the setup each of its diagnostics is about, and reports an entry's load diagnostics together"
)]
fn the_environment_says_what_each_diagnostic_is_about() {
    use specforge_project::SetupPart::{Collision, Peers, Providers, Rules};

    let dir = project_with(true);
    let runtime = Arc::new(colliding_extensions_with(true, true));

    let ctx = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime),
    );
    let setup = ctx.environment().setup_diagnostics();

    let reported: Vec<_> = setup
        .iter()
        .map(|s| (s.part, s.diagnostic.code.as_str()))
        .collect();
    assert_eq!(
        reported,
        [
            (Providers, "W118"),
            (Peers, "E027"),
            (Collision, "E026"),
            (Collision, "W018"),
            (Rules, "W112"),
            (Rules, "W112"),
            (Collision, "W023"),
            (Rules, "W112"),
            (Collision, "E039"),
        ]
    );
    // `diagnostics()` is the same list without the parts.
    let flat: Vec<_> = ctx.environment().diagnostics().collect();
    assert_eq!(flat, setup.iter().map(|s| s.diagnostic).collect::<Vec<_>>());
}
