use std::fs;
use std::path::Path;

use specforge_project::CompiledProject;
use specforge_project::testing::TestProject;
use specforge_test::prelude::*;
use std::sync::Arc;

fn project(config: serde_json::Value, files: &[(&str, &str)]) -> TestProject {
    TestProject::new().config(|c| *c = config).files(files)
}

/// [`project`] whose enabled extensions an in-process runtime serves: they
/// are installed too, as a project that enables them has them.
fn served_project(config: serde_json::Value, files: &[(&str, &str)]) -> TestProject {
    project(config, files).installing_enabled()
}

fn codes(diagnostics: &[specforge_common::Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

/// An extension that fails to load, a missing import and a duplicate ID:
/// one compile reports all three, each from its own layer, in `check`'s
/// order (environment, resolver, graph).
#[specforge_test(
    invariant = "multi_error_collection",
    verify = "the compiler does not halt after the first error"
)]
fn one_compile_reports_every_layers_errors() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/software", "@specforge/no-such-extension"]
        }),
        &[
            ("a.spec", "use \"missing\"\n\nterm alpha \"Alpha\" {\n}\n"),
            ("b.spec", "term alpha \"Alpha again\" {\n}\n"),
        ],
    );

    let compiled = dir.compile();
    let diagnostics = compiled.diagnostics();
    let codes = codes(&diagnostics);
    let position = |code: &str| {
        codes
            .iter()
            .position(|c| *c == code)
            .unwrap_or_else(|| panic!("no {code} in {codes:?}"))
    };
    assert!(position("E028") < position("E025"), "{codes:?}");
    assert!(position("E025") < position("E002"), "{codes:?}");
}

/// A compile keeps the spec root and the resolved files it read (MCP
/// needs both: plan 01, D8).
#[test]
fn the_context_view_keeps_the_spec_root_and_the_resolved_files() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/software"], "spec_root": "spec"
        }),
        &[("spec/a.spec", "term alpha \"Alpha\" {\n}\n")],
    );

    let compiled = dir.compile();

    assert_eq!(compiled.environment().spec_root(), dir.root().join("spec"));
    let mut files: Vec<String> = compiled.source_texts().into_keys().collect();
    files.sort();
    assert_eq!(files, ["a.spec"]);
}

/// A directory with no `specforge.json` at or above it is a loose root: the
/// compile still runs, structurally, and its I002 says no project was found.
#[specforge_test(
    behavior = "locate_project_root",
    verify = "a loose root's I002 says no project was found and suggests init"
)]
fn a_loose_root_says_no_project_was_found() {
    let dir = TestProject::new()
        .without_config()
        .file("a.spec", "thing alpha \"A\" {\n}\n");

    let diagnostics = dir.compile().diagnostics();

    assert_eq!(codes(&diagnostics), ["I002"], "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        format!(
            "no specforge.json at or above {} — operating in structural-only mode",
            dir.root().display()
        )
    );
    assert_eq!(
        diagnostics[0].suggestion.as_deref(),
        Some("create a project here with: specforge init")
    );
}

/// No extension configured: the compile still runs, structurally, and
/// says so once, with an I002 info.
#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "no extensions installed emits I002 info"
)]
fn a_project_without_extensions_reports_structural_only_mode() {
    let dir = project(
        serde_json::json!({ "name": "p", "version": "0.1.0" }),
        &[(
            "a.spec",
            "thing alpha \"A\" {\n  refs [beta]\n}\n\nthing beta \"B\" {\n}\n",
        )],
    );

    let diagnostics = dir.compile().diagnostics();

    assert_eq!(codes(&diagnostics), ["I002"], "{diagnostics:?}");
    assert_eq!(diagnostics[0].severity, specforge_common::Severity::Info);
    assert!(
        diagnostics[0].message.contains("no extensions configured"),
        "{diagnostics:?}"
    );
}

#[specforge_test(
    behavior = "graceful_degradation_without_extensions",
    verify = "Graceful Degradation Without Extensions: graceful degradation holds — registries_populated_fired, i002_emitted, structural_mode_operational, valid_export_produced"
)]
fn graceful_degradation_contract() {
    let dir = project(
        serde_json::json!({ "name": "p", "version": "0.1.0" }),
        &[(
            "a.spec",
            "thing alpha \"A\" {\n  refs [beta]\n}\n\nthing beta \"B\" {\n}\n",
        )],
    );

    let compiled = dir.compile();

    // registries_populated_fired: with no extension, the registries are
    // built, and empty.
    assert!(compiled.environment().registries().kinds().is_empty());
    // i002_emitted
    assert_eq!(codes(&compiled.diagnostics()), ["I002"]);
    // structural_mode_operational: generic nodes, raw keywords, a
    // reference edge.
    let alpha = compiled.graph().node("alpha").unwrap();
    assert_eq!(alpha.kind.raw.as_str(), "thing");
    assert_eq!(compiled.graph().edges_from("alpha").len(), 1);
    // valid_export_produced
    let json =
        specforge_emitter::emit(compiled.graph(), &specforge_emitter::EmitOptions::default())
            .unwrap();
    let exported: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(exported["nodes"].as_array().unwrap().len(), 2);
}

/// Every configured extension fails to load: one E028 each, then the
/// I002 that says the compile went on structurally.
#[specforge_test(
    behavior = "handle_all_extensions_failed_to_load",
    verify = "all extensions failing produces per-extension E-level diagnostics"
)]
fn every_failed_extension_gets_its_own_error() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/no-such-a", "@specforge/no-such-b"]
        }),
        &[("a.spec", "thing alpha \"A\" {\n}\n")],
    );

    let diagnostics = dir.compile().diagnostics();

    let errors: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E028")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(errors.len(), 2, "{diagnostics:?}");
    assert!(errors[0].contains("@specforge/no-such-a"), "{errors:?}");
    assert!(errors[1].contains("@specforge/no-such-b"), "{errors:?}");
}

#[specforge_test(
    behavior = "handle_all_extensions_failed_to_load",
    verify = "system transitions to structural-only mode after all failures"
)]
fn all_failed_extensions_leave_a_structural_compile() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/no-such-a", "@specforge/no-such-b"]
        }),
        &[(
            "a.spec",
            "thing alpha \"A\" {\n  refs [beta]\n}\n\nthing beta \"B\" {\n}\n",
        )],
    );

    let compiled = dir.compile();
    let diagnostics = compiled.diagnostics();

    // The graph is still built, and nothing kind-specific is reported.
    assert_eq!(compiled.graph().node_count(), 2);
    assert_eq!(
        codes(&diagnostics),
        ["E028", "E028", "I002"],
        "{diagnostics:?}"
    );
    assert!(
        diagnostics[2]
            .message
            .contains("none of the 2 configured extensions loaded"),
        "{diagnostics:?}"
    );
}

/// `@specforge/formal` requires `@specforge/software`: without it the
/// compile fails with E027, which says what to install.
#[specforge_test(
    invariant = "peer_dependency_satisfaction",
    verify = "unsatisfied peer dependency produces an error diagnostic"
)]
fn a_missing_required_peer_fails_the_compile() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/formal"]
        }),
        &[("a.spec", "term alpha \"Alpha\" {\n}\n")],
    );

    let diagnostics = dir.compile().diagnostics();

    let e027: Vec<_> = diagnostics.iter().filter(|d| d.code == "E027").collect();
    assert_eq!(e027.len(), 1, "{diagnostics:?}");
    assert!(
        e027[0].message.contains("'@specforge/formal'")
            && e027[0].message.contains("'@specforge/software'"),
        "{e027:?}"
    );
    assert_eq!(
        e027[0].suggestion.as_deref(),
        Some("install it with: specforge add @specforge/software")
    );
}

/// Optional peers may be absent: software (optional product) and testing
/// (optional software, governance) load alone without a peer error.
#[specforge_test(
    invariant = "peer_dependency_satisfaction",
    verify = "satisfied peer dependencies pass validation"
)]
fn missing_optional_peers_are_not_reported() {
    for extension in ["@specforge/software", "@specforge/testing"] {
        let dir = project(
            serde_json::json!({
                "name": "p", "version": "0.1.0", "extensions": [extension]
            }),
            &[("a.spec", "spec \"p\" {\n}\n")],
        );

        let diagnostics = dir.compile().diagnostics();

        assert!(
            !codes(&diagnostics).contains(&"E027"),
            "{extension}: {diagnostics:?}"
        );
    }
}

/// `feature` belongs to @specforge/product, which the project doesn't
/// enable: one E024 whose suggestion names product, and nothing else about
/// that keyword.
#[specforge_test(
    behavior = "resolve_soft_cross_extension_references",
    verify = "unknown keyword matching known extension gets E024 naming the extension"
)]
fn a_known_keyword_without_its_extension_is_one_e024_naming_it() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]
        }),
        &[("a.spec", "feature checkout \"Checkout\" {\n}\n")],
    );

    let diagnostics = dir.compile().diagnostics();

    // (software's own rules may add their warnings about the feature.)
    let e024: Vec<_> = diagnostics.iter().filter(|d| d.code == "E024").collect();
    assert_eq!(e024.len(), 1, "{diagnostics:?}");
    assert_eq!(
        e024[0].suggestion.as_deref(),
        Some("install it with: specforge add @specforge/product")
    );
    assert!(!codes(&diagnostics).contains(&"I004"), "{diagnostics:?}");
}

#[specforge_test(
    behavior = "resolve_soft_cross_extension_references",
    verify = "Resolve Soft Cross-Extension References: soft cross-extension resolution holds — registries_populated_fired, known_extensions_catalog_available, suggestion_emitted, installed_extensions_resolved"
)]
fn soft_cross_extension_resolution_contract() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]
        }),
        &[(
            "a.spec",
            "feature checkout \"Checkout\" {\n}\n\ninvariant alpha \"Alpha\" {\n  refs [missing]\n}\n",
        )],
    );

    let compiled = dir.compile();
    let diagnostics = compiled.diagnostics();

    // registries_populated_fired: software's kinds are registered.
    assert!(
        compiled
            .environment()
            .registries()
            .kinds()
            .contains("invariant")
    );
    // known_extensions_catalog_available, suggestion_emitted: the unknown
    // keyword's E024 names its extension, once.
    let e024: Vec<_> = diagnostics.iter().filter(|d| d.code == "E024").collect();
    assert_eq!(e024.len(), 1, "{diagnostics:?}");
    assert!(
        e024[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("@specforge/product")
    );
    assert!(!codes(&diagnostics).contains(&"I004"), "{diagnostics:?}");
    // installed_extensions_resolved: an installed kind's broken reference
    // is a plain E003.
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "E003" && d.message.contains("'missing'")),
        "{diagnostics:?}"
    );
}

/// Define blocks are not supported: each is one W143, and none reaches the
/// graph, so its body is not read as references (no E003) and its name is
/// not checked as an ID (no E013 for `define behavior`).
#[specforge_test(
    behavior = "report_define_blocks",
    verify = "a define block produces one W143 and adds no node to the graph"
)]
fn a_define_block_is_one_warning_and_no_node() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]
        }),
        &[(
            "a.spec",
            "define user_story {\n  required [title]\n}\n\ndefine behavior {\n}\n",
        )],
    );

    let compiled = dir.compile();
    let diagnostics = compiled.diagnostics();

    assert_eq!(codes(&diagnostics), ["W143", "W143"], "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("'user_story'"));
    assert!(diagnostics[1].message.contains("'behavior'"));
    assert!(
        diagnostics[0]
            .suggestion
            .as_deref()
            .unwrap()
            .contains("extension")
    );
    assert_eq!(compiled.graph().node_count(), 0);
}

/// A project with an extension loaded is not in structural-only mode.
#[test]
fn a_loaded_extension_means_no_i002() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/software", "@specforge/no-such-extension"]
        }),
        &[("a.spec", "term alpha \"Alpha\" {\n}\n")],
    );

    let diagnostics = dir.compile().diagnostics();

    assert!(!codes(&diagnostics).contains(&"I002"), "{diagnostics:?}");
}

/// Provider extensions, in process: each contributes providers (a raw
/// `providers` category, which raises the handshake flag) and nothing
/// else. No builtin contributes providers.
fn provider_extensions(
    names: &'static [&'static str],
) -> specforge_wasm::testing::InProcessRuntime {
    names.iter().fold(
        specforge_wasm::testing::InProcessRuntime::new(),
        |runtime, name| {
            runtime.with(move || {
                let mut b = specforge_extension_sdk::ContributionsBuilder::new(
                    specforge_extension_sdk::ExtensionMeta::new(name, "1.0.0"),
                );
                b.providers();
                b
            })
        },
    )
}

/// The compile's diagnostics, without the W012 every unreferenced ref
/// gets.
fn compile_with_providers(
    dir: &TestProject,
    extensions: &'static [&'static str],
) -> Vec<specforge_common::Diagnostic> {
    dir.compile_in(Arc::new(provider_extensions(extensions)))
        .diagnostics()
        .into_iter()
        .filter(|d| d.code != "W012")
        .collect()
}

/// A configured provider registers its scheme: a ref with that scheme is
/// known, and a ref with another scheme is I005.
#[specforge_test(
    behavior = "register_provider_schemes",
    verify = "Wasm-based provider scheme registered and validates ref"
)]
fn a_registered_provider_scheme_validates_refs() {
    let dir = served_project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@test/gh-provider"],
            "providers": [
                { "scheme": "gh", "alias": "main", "extension": "@test/gh-provider" }
            ]
        }),
        &[(
            "a.spec",
            "ref gh.issue:42 \"Known scheme\"\nref jira.story:7 \"Unknown scheme\"\n",
        )],
    );

    let diagnostics = compile_with_providers(&dir, &["@test/gh-provider"]);

    assert_eq!(codes(&diagnostics), ["I005"], "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("'jira'"), "{diagnostics:?}");
}

/// Two providers declaring one scheme: E057 reaches `check`, and the first
/// declared keeps the scheme.
#[specforge_test(
    behavior = "register_provider_schemes",
    verify = "duplicate scheme from two providers produces E057"
)]
fn a_scheme_declared_twice_is_e057_on_compile() {
    let dir = served_project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@test/gh-a", "@test/gh-b"],
            "providers": [
                { "scheme": "gh", "alias": "a", "extension": "@test/gh-a" },
                { "scheme": "gh", "alias": "b", "extension": "@test/gh-b" }
            ]
        }),
        &[("a.spec", "ref gh.issue:42 \"Known scheme\"\n")],
    );

    let diagnostics = compile_with_providers(&dir, &["@test/gh-a", "@test/gh-b"]);

    assert_eq!(codes(&diagnostics), ["E057"], "{diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("already registered by provider 'a'"),
        "{diagnostics:?}"
    );
}

/// Without a provider configured, refs are not checked for their scheme.
#[test]
fn no_provider_means_no_scheme_check() {
    let dir = served_project(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": ["@test/gh-provider"]
        }),
        &[("a.spec", "ref jira.story:7 \"Unknown scheme\"\n")],
    );

    let diagnostics = compile_with_providers(&dir, &["@test/gh-provider"]);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A compile keeps the text it parsed: a file rewritten on disk after the
/// compile is still quoted as it was compiled.
#[test]
fn source_texts_are_what_was_compiled() {
    let compiled_text = "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n";
    let dir = project(
        serde_json::json!({"name": "p", "version": "0.1.0", "extensions": []}),
        &[("a.spec", compiled_text), ("sub/b.spec", "// b\n")],
    );
    let compiled = dir.compile_bare();
    fs::write(dir.root().join("a.spec"), "// rewritten\n").unwrap();

    let texts = compiled.source_texts();
    assert_eq!(&*texts["a.spec"], compiled_text);
    assert_eq!(&*texts["sub/b.spec"], "// b\n");
    assert_eq!(texts.len(), 2, "{texts:?}");
}

/// Compiling an environment already loaded is compiling its root: the CLI
/// routes an extension command on the environment, then compiles from it.
#[test]
fn a_compile_of_a_loaded_environment_is_the_compile_of_its_root() {
    let dir = project(
        serde_json::json!({"name": "p", "version": "0.1.0", "extensions": ["@specforge/product"]}),
        &[(
            "main.spec",
            "feature f1 \"One\" {\n  status done\n  problem \"p\"\n}\n\nfeature f1 \"Again\" {\n}\n",
        )],
    );
    let runtime = Arc::new(specforge_component::ComponentRuntime::new());
    let whole = dir.compile_in(runtime.clone());
    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
        Some(runtime),
    );
    let of = CompiledProject::of(env);

    assert_eq!(of.diagnostics(), whole.diagnostics());
    assert_eq!(
        specforge_emitter::emit(of.graph(), &specforge_emitter::EmitOptions::default()).unwrap(),
        specforge_emitter::emit(whole.graph(), &specforge_emitter::EmitOptions::default()).unwrap()
    );
    assert_eq!(of.source_texts(), whole.source_texts());
    assert!(
        !of.diagnostics().is_empty(),
        "the project has an error to compare"
    );
}

/// Characterization (plan 03 T0): the order `Environment::diagnostics()`
/// reports a missing extension's load failure, the loaded extensions'
/// declaration diagnostics and the registry build's. The registry build
/// owning declaration validation changes it (ADR 0012).
#[test]
fn environment_diagnostics_come_in_load_order() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@specforge/formal", "@acme/missing", "@specforge/product"]
        }),
        &[],
    );
    let runtime = dir.runtime();
    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
        Some(runtime.clone()),
    );
    let codes: Vec<&str> = env.diagnostics().map(|d| d.code.as_str()).collect();
    // The missing extension's load failure, formal's missing peer, then the
    // registry build's: formal's enhancements of software's kinds.
    assert_eq!(
        codes,
        ["E028", "E027", "I004", "I004", "I004"],
        "{:#?}",
        env.diagnostics().collect::<Vec<_>>()
    );
}

mod declared_in_process {
    //! Extensions declared with the SDK and served in process: what they
    //! declare is what the environment loads.

    use super::served_project as project;
    use specforge_extension_sdk::prelude::*;
    use specforge_project::Environment;
    use specforge_wasm::testing::InProcessRuntime;
    use std::sync::Arc;

    fn reports() -> ContributionsBuilder {
        let mut meta = ExtensionMeta::new("@acme/reports", "0.1.0");
        meta.short = Some("rep".to_string());
        let mut b = ContributionsBuilder::new(meta);
        b.kind("report", |k| {
            k.description("A report");
        });
        b.command("list", |c| {
            c.title("List")
                .description("List reports")
                .handler(|_| CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
        });
        b
    }

    fn commands_only() -> ContributionsBuilder {
        let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/cmds", "0.1.0"));
        b.command("hello", |c| {
            c.title("Hello")
                .description("Say hello")
                .handler(|_| CommandOutput {
                    exit_code: 0,
                    stdout: "hi".to_string(),
                    stderr: String::new(),
                });
        });
        b
    }

    fn load(extensions: &[&str], runtime: Arc<InProcessRuntime>) -> Environment {
        let dir = project(
            serde_json::json!({ "name": "p", "version": "0.1.0", "extensions": extensions }),
            &[],
        );
        Environment::load(
            &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
            Some(runtime),
        )
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_declaration",
        verify = "an extension that only declares commands registers its commands"
    )]
    fn a_commands_only_extension_registers_its_commands() {
        let runtime = Arc::new(InProcessRuntime::new().with(commands_only));
        let env = load(&["@acme/cmds"], runtime);
        let registered: Vec<&str> = env
            .registries()
            .commands()
            .routed()
            .iter()
            .map(|r| r.declaration().id.as_str())
            .collect();
        assert!(registered.contains(&"hello"), "{registered:?}");
        let errors: Vec<_> = env
            .diagnostics()
            .filter(|d| d.severity == specforge_common::Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_declaration",
        verify = "the declared short name reaches the registry build"
    )]
    fn the_declared_short_name_reaches_the_registry_build() {
        let runtime = Arc::new(InProcessRuntime::new().with(reports));
        let env = load(&["@acme/reports"], runtime);
        let declaration = env
            .registries()
            .declaration("@acme/reports")
            .expect("loaded");
        assert_eq!(declaration.short(), "rep");
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_declaration",
        verify = "an unsupported protocol major version fails the load"
    )]
    fn an_unsupported_protocol_major_fails_the_load() {
        let handshake = serde_json::to_vec(&specforge_protocol_types::HandshakeResponse {
            protocol_version: "2.0.0".to_string(),
            name: "@acme/reports".to_string(),
            version: "0.1.0".to_string(),
            ..Default::default()
        })
        .unwrap();
        let runtime = Arc::new(InProcessRuntime::new().with(reports).answer_raw(
            "@acme/reports",
            "__handshake",
            specforge_wasm::WasmCallResult::Ok(handshake),
        ));
        let env = load(&["@acme/reports"], runtime);
        assert!(env.registries().declaration("@acme/reports").is_none());
        assert!(!env.registries().kinds().contains("report"));
        let e028: Vec<_> = env.diagnostics().filter(|d| d.code == "E028").collect();
        assert_eq!(e028.len(), 1, "{e028:?}");
        assert!(e028[0].message.contains("@acme/reports"), "{e028:?}");
        assert!(e028[0].message.contains("2.0.0"), "{e028:?}");
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_declaration",
        verify = "a describe item key the protocol does not define produces W138"
    )]
    fn an_unknown_describe_key_produces_a_warning() {
        let typo = || {
            let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/typo", "0.1.0"));
            b.raw_category(
                specforge_extension_sdk::DeclaredCategory::Entities,
                serde_json::json!([{ "name": "memo", "testabel": true }]),
            );
            b
        };
        let runtime = Arc::new(InProcessRuntime::new().with(typo));
        let env = load(&["@acme/typo"], runtime);
        // The extension still loads, without the key it misspelled.
        assert!(env.registries().kinds().contains("memo"));
        let warnings: Vec<_> = env
            .diagnostics()
            .filter(|d| d.severity == specforge_common::Severity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].code, "W138");
        assert!(warnings[0].message.contains("'testabel'"), "{warnings:?}");
    }
}

mod passes_of_the_declaration {
    //! The passes an environment runs are the ones its one declaration load
    //! read: nothing describes `passes` again, and a passes answer that does
    //! not parse fails the extension's load.

    use super::served_project as project;
    use specforge_extension_sdk::prelude::*;
    use specforge_project::Environment;
    use specforge_wasm::WasmCallResult;
    use specforge_wasm::testing::InProcessRuntime;
    use std::sync::Arc;

    fn audit() -> ContributionsBuilder {
        let mut b = ContributionsBuilder::new(ExtensionMeta::new("@acme/audit", "0.1.0"));
        b.pass("audit", |p| {
            p.phase("check")
                .run(|_: &PassInput| Vec::<PassDiagnostic>::new());
        });
        b
    }

    /// The audit extension, whose `passes` answer is `items` instead of
    /// what it declares (no SDK guest can answer a description that does
    /// not parse, so it is given raw).
    fn malformed_passes(items: serde_json::Value) -> InProcessRuntime {
        let answer = serde_json::json!({ "category": "passes", "items": items });
        InProcessRuntime::new().with(audit).answer_raw_to(
            "@acme/audit",
            "__describe",
            serde_json::json!({ "category": "passes" }),
            WasmCallResult::Ok(answer.to_string().into_bytes()),
        )
    }

    #[specforge_test_macros::test(
        behavior = "build_registries_from_declarations",
        verify = "a passes description that does not parse fails the extension's load"
    )]
    fn a_passes_description_that_does_not_parse_fails_the_load() {
        let runtime = Arc::new(malformed_passes(serde_json::json!([{ "nam": "x" }])));
        let dir = project(
            serde_json::json!({ "name": "p", "version": "0.1.0", "extensions": ["@acme/audit"] }),
            &[],
        );
        let env = Environment::load(
            &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
            Some(runtime.clone()),
        );
        let e028: Vec<&str> = env
            .diagnostics()
            .filter(|d| d.code == "E028")
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(e028.len(), 1, "{e028:?}");
        assert!(
            e028[0].contains("describe 'passes' failed")
                && e028[0].contains("missing field `name`"),
            "{}",
            e028[0]
        );
        assert_eq!(env.registries().check_passes().count(), 0);
    }

    /// One environment load describes each category of each extension
    /// once; compiling with it runs the check passes it read, describing
    /// nothing again.
    #[test]
    fn an_environment_describes_each_category_once() {
        let runtime = Arc::new(InProcessRuntime::new().with(audit));
        let dir = project(
            serde_json::json!({ "name": "p", "version": "0.1.0", "extensions": ["@acme/audit"] }),
            &[("a.spec", "spec p \"P\" {\n}\n")],
        );
        let compiled = dir.compile_in(runtime.clone());
        let calls: Vec<String> = runtime
            .calls()
            .iter()
            .map(|c| match c.input["category"].as_str() {
                Some(category) => format!("{}:{category}", c.export),
                None => c.export.clone(),
            })
            .collect();
        let describes = calls.iter().filter(|c| c.starts_with("__describe")).count();
        assert_eq!(
            describes,
            specforge_protocol_types::DECLARED_CATEGORIES.len(),
            "{calls:?}"
        );
        assert_eq!(calls.iter().filter(|c| *c == "__handshake").count(), 1);
        assert_eq!(calls.last().map(String::as_str), Some("__pass_audit"));
        assert!(
            compiled.diagnostics().iter().all(|d| d.code != "E028"),
            "{:?}",
            compiled.diagnostics()
        );
    }

    fn no_version() -> ContributionsBuilder {
        ContributionsBuilder::new(ExtensionMeta::new("@acme/noversion", ""))
    }

    /// The runtime's load failures (E028) come before the declarations'
    /// own diagnostics (E030), whatever the load order (ADR 0012): they
    /// used to interleave extension by extension.
    #[test]
    fn load_failures_come_before_declaration_diagnostics() {
        let runtime = Arc::new(InProcessRuntime::new().with(no_version));
        let dir = project(
            serde_json::json!({
                "name": "p", "version": "0.1.0",
                "extensions": ["@acme/noversion", "@acme/missing"]
            }),
            &[],
        );
        let env = Environment::load(
            &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
            Some(runtime.clone()),
        );
        let codes: Vec<&str> = env.diagnostics().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["E028", "E030"], "{codes:?}");
    }
}

/// A `specforge.json` that is there and can't be used: E069 first (an
/// error), then an I002 that names the file, not "no extensions
/// configured".
#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "a specforge.json that is there and can't be used produces E069 first and an I002 that names it"
)]
fn an_unusable_config_is_e069_then_i002_naming_it() {
    let dir = TestProject::new().raw_config(r#"{ "extensions": ["#);
    let runtime = dir.runtime();

    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
        Some(runtime.clone()),
    );

    let diagnostics: Vec<&specforge_common::Diagnostic> = env.diagnostics().collect();
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E069", "I002"], "{diagnostics:?}");
    assert_eq!(diagnostics[0].severity, specforge_common::Severity::Error);
    assert!(
        diagnostics[0]
            .message
            .starts_with("specforge.json can't be used: "),
        "{}",
        diagnostics[0].message
    );
    assert!(
        diagnostics[0].message.ends_with("; no extension is loaded"),
        "{}",
        diagnostics[0].message
    );
    assert_eq!(
        diagnostics[1].message,
        "specforge.json could not be read — operating in structural-only mode"
    );
    assert_eq!(
        diagnostics[1].suggestion.as_deref(),
        Some("fix specforge.json (E069 above)")
    );
    assert!(
        matches!(
            env.config_problems(),
            [specforge_common::ConfigProblem::NotJson { .. }]
        ),
        "{:?}",
        env.config_problems()
    );
    assert!(env.config_found());

    // No file at all: no problem, and the notice names the loose root.
    let empty = TestProject::new().without_config();
    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(empty.root()).unwrap(),
        Some(runtime.clone()),
    );
    assert!(!env.config_found());
    assert!(env.config_problems().is_empty());
    let messages: Vec<&str> = env.diagnostics().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [format!(
            "no specforge.json at or above {} — operating in structural-only mode",
            empty.root().display()
        )]
    );
}

/// The environment reads `specforge.lock` once, at its root: no file is
/// absent, a lock is read as written, a file that cannot be parsed is
/// unreadable with its E033 problem (and no extension is locked).
#[test]
fn the_environment_reads_the_lock_once_at_its_root() {
    use specforge_installed::{LockFile, LockState};
    let dir = TestProject::new().raw_config("{}");
    let runtime = dir.runtime();
    let load = || {
        specforge_project::Environment::load(
            &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
            Some(runtime.clone()),
        )
    };

    assert_eq!(load().installed().lock(), &LockState::Absent);

    let lock = LockFile::default();
    specforge_installed::write_lock_file(&lock, &specforge_installed::lock_path(dir.root()))
        .unwrap();
    assert_eq!(load().installed().lock(), &LockState::Read(lock));

    // What changed on disk after the load is not what the environment holds.
    let env = load();
    fs::write(
        specforge_installed::lock_path(dir.root()),
        "not valid json {{{",
    )
    .unwrap();
    assert!(matches!(env.installed().lock(), LockState::Read(_)));
    let reloaded = load();
    assert_eq!(
        reloaded
            .installed()
            .lock()
            .problem()
            .map(|p| p.code.as_str()),
        Some("E033")
    );
    assert!(reloaded.installed().lock().entries().is_empty());
}

/// A non-string `extensions` item is one E069 (it is ignored); the other
/// entries load, so there is no I002.
#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
)]
fn a_non_string_extension_entry_is_e069_and_the_others_load() {
    let dir = project(
        serde_json::json!({"extensions": ["@specforge/product", 42]}),
        &[("a.spec", "feature f \"F\" {\n}\n")],
    );

    let compiled = dir.compile();

    let diagnostics = compiled.diagnostics();
    let e069: Vec<&specforge_common::Diagnostic> =
        diagnostics.iter().filter(|d| d.code == "E069").collect();
    assert_eq!(e069.len(), 1, "{diagnostics:?}");
    assert_eq!(
        e069[0].message,
        format!(
            "specforge.json: \"extensions\"[1] in {} must be a string, found 42; it is ignored",
            dir.root().join("specforge.json").display()
        )
    );
    assert_eq!(codes(&diagnostics)[0], "E069", "E069 is reported first");
    assert!(!codes(&diagnostics).contains(&"I002"), "{diagnostics:?}");
    assert!(
        compiled
            .environment()
            .registries()
            .kinds()
            .get("feature")
            .is_some(),
        "product loaded: its kinds are registered"
    );
}

/// A `.spec` file discovery finds and that cannot be read is E025 naming
/// its path, and is left out of the graph.
#[specforge_test(
    behavior = "resolve_use_imports",
    verify = "a source that cannot be read produces E025 naming it"
)]
fn an_unreadable_source_is_e025_naming_it() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0", "extensions": ["@specforge/software"]
        }),
        &[("a.spec", "term alpha \"Alpha\" {\n}\n")],
    );
    fs::write(
        dir.root().join("bad.spec"),
        b"term beta \"B\xff\xfe\" {\n}\n",
    )
    .unwrap();

    let compiled = dir.compile();

    let e025: Vec<_> = compiled
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "E025")
        .collect();
    assert_eq!(e025.len(), 1, "{e025:?}");
    assert_eq!(
        e025[0].message,
        "cannot read bad.spec: stream did not contain valid UTF-8"
    );
    assert!(e025[0].span.is_none());
    let ids: Vec<_> = compiled
        .graph()
        .nodes()
        .iter()
        .map(|n| n.id.raw.to_string())
        .collect();
    assert_eq!(ids, ["alpha"]);
    assert_eq!(compiled.source_texts().len(), 1);
}

/// `@sdk/greet` installed by hand under `root`: its module, and a lock
/// entry pinning `hash` (the module's own when `None`).
fn install_greet(root: &Path, hash: Option<&str>) {
    let blob = specforge_component::committed::GREET.path();
    let bytes = fs::read(blob).unwrap();
    let module = root.join(".specforge/extensions/@sdk/greet/extension.wasm");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(&module, &bytes).unwrap();
    let lock = specforge_installed::LockFile {
        lockfile_version: 1,
        entries: vec![specforge_installed::LockFileEntry {
            name: specforge_package::PackageName::parse("@sdk/greet").unwrap(),
            version: "0.1.0".into(),
            source: specforge_installed::LockSource::Registry,
            wasm_hash: hash.map_or_else(|| specforge_installed::hex_sha256(&bytes), str::to_string),
            key_id: None,
            peer_dependencies: Vec::new(),
        }],
    };
    specforge_installed::write_lock_file(&lock, &specforge_installed::lock_path(root)).unwrap();
}

/// A `specforge.json` entry that names no package is E072 and is not
/// loaded: no module is looked for under a path it would escape.
#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "an extension is installed under the extensions directory of its project, by its package name"
)]
fn an_entry_that_names_no_package_is_e072_and_not_loaded() {
    let dir = project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["../../../outside1", "@specforge/product"]
        }),
        &[],
    );
    let runtime = dir.component_runtime();

    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
        Some(runtime.clone()),
    );

    let e072: Vec<&specforge_common::Diagnostic> =
        env.diagnostics().filter(|d| d.code == "E072").collect();
    assert_eq!(e072.len(), 1, "{:?}", env.diagnostics().collect::<Vec<_>>());
    assert!(e072[0].message.contains("../../../outside1"), "{e072:?}");
    assert_eq!(runtime.loaded_names(), ["@specforge/product"]);
}

/// A lock that cannot be read is reported once (E033), then each installed
/// extension it leaves unloaded is E028 naming it.
#[specforge_test(
    behavior = "load_extension_manifests",
    verify = "an unreadable specforge.lock is reported once (E033) and each installed extension it leaves unloaded is E028 naming it"
)]
fn an_unreadable_lock_reports_e033_once_then_each_installed_extension() {
    let dir = project(
        serde_json::json!({"name": "p", "version": "0.1.0", "extensions": ["@sdk/greet"]}),
        &[],
    );
    install_greet(dir.root(), None);
    fs::write(
        specforge_installed::lock_path(dir.root()),
        "not valid json {{{",
    )
    .unwrap();
    let runtime = dir.runtime();

    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
        Some(runtime.clone()),
    );

    let diagnostics: Vec<&specforge_common::Diagnostic> = env.diagnostics().collect();
    let lock: Vec<&&specforge_common::Diagnostic> =
        diagnostics.iter().filter(|d| d.code == "E033").collect();
    assert_eq!(lock.len(), 1, "{diagnostics:?}");
    assert!(lock[0].message.contains("corrupt lock file"));
    let unloaded: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == "E028" && d.message.contains("@sdk/greet"))
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(unloaded.len(), 1, "{diagnostics:?}");
    assert!(unloaded[0].contains("specforge.lock can't be read"));
    let at = |code: &str| diagnostics.iter().position(|d| d.code == code).unwrap();
    assert!(at("E033") < at("E028"), "the lock comes first");
}

/// A lock entry that pins no hash loads its extension, with W149.
#[specforge_test(
    behavior = "load_wasm_module",
    verify = "legacy lockfile entry without hash loads with W149"
)]
fn an_unpinned_lock_entry_loads_with_w149() {
    let dir = project(
        serde_json::json!({"name": "p", "version": "0.1.0", "extensions": ["@sdk/greet"]}),
        &[],
    );
    install_greet(dir.root(), Some(""));
    let runtime = dir.runtime();

    let env = specforge_project::Environment::load(
        &specforge_common::ProjectRoot::of(dir.root()).unwrap(),
        Some(runtime.clone()),
    );

    let named: Vec<(&str, specforge_common::Severity)> = env
        .diagnostics()
        .filter(|d| d.message.contains("@sdk/greet"))
        .map(|d| (d.code.as_str(), d.severity))
        .collect();
    assert_eq!(named, [("W149", specforge_common::Severity::Warning)]);
    assert!(
        env.registries()
            .declarations()
            .iter()
            .any(|d| d.name() == "@sdk/greet"),
        "its declaration loaded"
    );
}

/// `name`, declaring the kind `widget`, with a required peer on `peer` when given.
fn widget_extension(
    name: &'static str,
    peer: Option<&'static str>,
) -> impl Fn() -> specforge_extension_sdk::ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut meta = specforge_extension_sdk::ExtensionMeta::new(name, "1.0.0");
        if let Some(peer) = peer {
            meta.peer_dependencies = vec![specforge_extension_sdk::PeerDependency {
                name: peer.to_string(),
                version: "^1".to_string(),
                optional: false,
            }];
        }
        let mut c = specforge_extension_sdk::ContributionsBuilder::new(meta);
        c.kind("Widget", |k| {
            k.keyword("widget");
        });
        c
    }
}

#[specforge_test(
    invariant = "extension_load_order_determinism",
    verify = "the same extensions give the same load order on every build, dependencies first"
)]
fn a_project_listing_a_dependent_first_loads_its_peer_first() {
    let dir = served_project(
        serde_json::json!({
            "name": "p", "version": "0.1.0",
            "extensions": ["@acme/dep", "@acme/base"]
        }),
        &[],
    );
    let runtime = Arc::new(
        specforge_wasm::testing::InProcessRuntime::new()
            .with(widget_extension("@acme/dep", Some("@acme/base")))
            .with(widget_extension("@acme/base", None)),
    );

    let compiled = dir.compile_in(runtime.clone());

    let e026: Vec<String> = compiled
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "E026")
        .map(|d| d.message)
        .collect();
    assert_eq!(e026.len(), 1, "{e026:?}");
    assert!(
        e026[0].contains("registered by '@acme/dep' conflicts with '@acme/base'"),
        "{e026:?}"
    );
    let loaded: Vec<&str> = compiled
        .environment()
        .registries()
        .declarations()
        .iter()
        .map(|d| d.name())
        .collect();
    assert_eq!(loaded, ["@acme/base", "@acme/dep"]);
    let enabled: Vec<&str> = compiled
        .environment()
        .enabled()
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(enabled, ["@acme/dep", "@acme/base"]);
}
