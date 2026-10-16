//! The management operations (the extensions and providers listings,
//! doctor, remove, collect, inference progress and gaps) read the project
//! from their view (ADR 0015, "Management operations").

use specforge_common::Diagnostic;
use specforge_ops::analysis::collect::{Consent, Mode, Request};
use specforge_ops::extension::{self, RemoveRequest, Status};
use specforge_ops::testing::ScriptedSolver;
use specforge_ops::view::ProjectView;
use specforge_project::testing::compile_at;
use specforge_project::{EnabledExtension, Environment, ExtensionLoad, Loaded};
use specforge_test::prelude::*;
use specforge_wasm::testing::InProcessRuntime;

use crate::view_support::{Assembled, Viewed, config_read};

/// A project whose compile read `specforge.json` as enabling
/// `@acme/missing@1.2.0` and configuring one provider, with one lock entry
/// at its root, while the `specforge.json` on disk now says something
/// else entirely.
fn project() -> Assembled {
    let config = serde_json::json!({
        "extensions": ["@acme/missing@1.2.0"],
        "providers": [{"scheme": "gh", "alias": "work", "extension": "@acme/issues"}],
    });
    Assembled::assembling(|root| {
        let read = config_read(root, config);
        // The specforge.json on disk now says something else entirely, and
        // the lock holds one entry.
        std::fs::write(
            root.join("specforge.json"),
            r#"{"extensions": ["@other/x@9.9.9"], "providers": []}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("specforge.lock"),
            serde_json::json!({"lockfile_version": 1, "entries": [
                {"name": "@acme/locked", "version": "1.0.0", "source": "registry", "wasm_hash": "h"}
            ]})
            .to_string(),
        )
        .unwrap();
        Environment::assemble(
            read,
            // The compile read the lock as it is at this root.
            specforge_installed::Installed::at(root),
            ExtensionLoad::In {
                runtime: std::sync::Arc::new(InProcessRuntime::new()),
                loaded: Loaded {
                    // It did not load: not installed, as the load recorded it.
                    enabled: vec![EnabledExtension {
                        failure: Some(specforge_installed::LoadFailure {
                            problem: specforge_installed::LoadProblem::NotInstalled,
                            diagnostic: Diagnostic::new(
                                specforge_common::codes::E028,
                                "extension '@acme/missing' is not installed",
                            ),
                        }),
                        ..EnabledExtension::unloaded("@acme/missing@1.2.0")
                    }],
                    declarations: Vec::new(),
                },
            },
        )
    })
    .source("behavior a \"A\" {\n}\n")
}

#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "Management Operations over the Project View: management operations hold — project_compiled, one_project_read, root_for_disk"
)]
fn management_operations_read_the_project_from_their_view() {
    // project_compiled: the view is what a compile supplied.
    let project = project();
    let e028 = [Diagnostic::new(
        specforge_common::codes::E028,
        "extension '@acme/missing' is not installed",
    )];
    let view = project.view().reporting(&e028);

    // one_project_read: the config, what each entry enabled and what the
    // surface reports come from the view, never specforge.json again.
    let listing = extension::list(&view);
    let listed: Vec<(&str, Status, Option<&str>)> = listing
        .extensions
        .iter()
        .map(|e| (e.name.as_str(), e.status, e.version.as_deref()))
        .collect();
    assert_eq!(
        listed,
        [
            ("@acme/locked", Status::NotConfigured, Some("1.0.0")),
            ("@acme/missing", Status::NotLoaded, Some("1.2.0")),
        ]
    );
    let providers = extension::providers(&view);
    assert_eq!(providers.providers.len(), 1, "{providers:?}");
    assert_eq!(providers.providers[0].alias, "work");

    let report = specforge_ops::analysis::doctor::diagnose(&view, &ScriptedSolver::ready());
    let failures: Vec<&str> = report
        .about(specforge_ops::analysis::doctor::Part::Load)
        .map(|f| f.code.as_str())
        .collect();
    assert_eq!(failures, ["E028"]);

    // root_for_disk: the lock is read at the view's root ...
    assert_eq!(listing.locked.len(), 1);
    assert_eq!(report.installed_count, 1);

    // ... and a view without one: the listings and doctor answer from what
    // the view enabled and loaded; the others refuse with no_project.
    let rootless = project.rootless_view().reporting(&e028);
    let listing = extension::list(&rootless);
    assert!(listing.locked.is_empty());
    assert_eq!(listing.extensions.len(), 1);
    assert_eq!(
        specforge_ops::analysis::doctor::diagnose(&rootless, &ScriptedSolver::ready())
            .installed_count,
        0
    );
    let refused = |result: Result<(), specforge_ops::OpError>| {
        assert_eq!(result.unwrap_err().code, "no_project");
    };
    refused(
        extension::remove(
            &rootless,
            &RemoveRequest {
                name: "@acme/missing",
                force: false,
                dry_run: true,
                containment: &specforge_ops::Containment::unbounded(),
            },
        )
        .map(drop)
        .map_err(specforge_ops::Refusal::into_error),
    );
    refused(specforge_ops::read::infer::progress(&rootless).map(drop));
    refused(specforge_ops::read::infer::gaps(&rootless).map(drop));
    refused(
        specforge_ops::analysis::collect::collect(
            &rootless,
            Request {
                runner: None,
                mode: Mode::NoRun,
                consent: Consent::Approved,
                announce: &mut |_, _| {},
            },
        )
        .map(drop)
        .map_err(specforge_ops::Refusal::into_error),
    );
    assert!(
        !project.root().join("specforge-report.json").exists(),
        "nothing was written"
    );
}

/// The providers listing reports what the environment registered when the
/// project compiled: its entries, and the W118 the registration reported.
#[test]
fn the_providers_listing_reports_what_the_environment_registered() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "p", "version": "0.1.0", "extensions": [],
        "providers": [
            {"alias": "a", "extension": "@acme/x"},
            {"scheme": "gh", "alias": "b", "extension": "@acme/absent"},
        ],
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    let compiled = compile_at(dir.path(), None);

    let listing = extension::providers(&ProjectView::of(&compiled));

    let entries: Vec<(&str, &str, &str, &str)> = listing
        .providers
        .iter()
        .map(|p| {
            (
                p.scheme.as_str(),
                p.alias.as_str(),
                p.extension.as_str(),
                p.status.as_str(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        [("gh", "b", "@acme/absent", "extension_not_loaded")]
    );
    let w118 = |diagnostics: &[Diagnostic]| -> Vec<(String, String)> {
        diagnostics
            .iter()
            .filter(|d| d.code == "W118")
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect()
    };
    assert_eq!(w118(&listing.diagnostics).len(), 2, "{listing:?}");
    assert_eq!(
        w118(&listing.diagnostics),
        w118(
            &compiled
                .environment()
                .providers()
                .diagnostics()
                .cloned()
                .collect::<Vec<_>>()
        )
    );
}

#[specforge_test(
    behavior = "register_provider_schemes",
    verify = "the providers listing reads the environment's registration, never specforge.json again"
)]
fn the_providers_listing_reads_the_registration() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "p", "version": "0.1.0", "extensions": [],
        "providers": [{"scheme": "gh", "alias": "work", "extension": "@acme/issues"}],
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    let compiled = compile_at(dir.path(), None);

    // specforge.json now says something else entirely.
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "providers": []}"#,
    )
    .unwrap();

    let listing = extension::providers(&ProjectView::of(&compiled));
    let aliases: Vec<&str> = listing.providers.iter().map(|p| p.alias.as_str()).collect();
    assert_eq!(aliases, ["work"], "{listing:?}");
    assert_eq!(listing.diagnostics.len(), 1, "{listing:?}");
}

#[specforge_test(
    behavior = "report_command_outcome",
    verify = "a command run outside any project refuses with no_project"
)]
fn collect_at_a_root_that_is_no_project_is_no_project() {
    let project = project();
    // A project whose environment found no specforge.json.
    let bare = Assembled::new()
        .without_config_file()
        .graph(project.compiled().graph().clone());
    let view = bare.view();

    let error = specforge_ops::analysis::collect::collect(
        &view,
        Request {
            runner: None,
            mode: Mode::NoRun,
            consent: Consent::Approved,
            announce: &mut |_, _| {},
        },
    )
    .map(drop)
    .unwrap_err();

    assert_eq!(error.error.code, "no_project", "{error:?}");
    assert!(
        error.error.message.contains("no specforge project at"),
        "{error:?}"
    );
    assert_eq!(
        std::fs::read_dir(bare.root()).unwrap().count(),
        0,
        "nothing was written"
    );
}

#[specforge_test(
    behavior = "list_installed_extensions",
    verify = "an extension's source is where the extension load takes it from, whatever order its entries are in"
)]
fn an_extensions_source_is_the_entry_that_loaded_it() {
    use specforge_ops::extension::{Origin, inventory};

    let dir = tempfile::tempdir().unwrap();
    let config = serde_json::json!({
        "name": "p", "version": "0.1.0", "extensions": ["@sdk/greet", "greet.wasm"],
    });
    std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    std::fs::write(dir.path().join("greet.wasm"), specforge_ops::testing::GREET).unwrap();

    // The named entry is not installed (no lock) and fails; the file entry loads it.
    let compiled = compile_at(
        dir.path(),
        Some(std::sync::Arc::new(specforge_ops::testing::candidates())),
    );
    let view = ProjectView::of(&compiled);
    let rows = inventory(&view);
    let greet = rows.get("@sdk/greet").expect("the row of @sdk/greet");

    assert_eq!(greet.status, Status::Loaded);
    assert_eq!(
        greet.origin,
        Origin::File {
            path: "greet.wasm".into()
        }
    );
}
