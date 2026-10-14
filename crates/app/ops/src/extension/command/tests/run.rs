use super::*;

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "args serialized as JSON to cmd__ export"
)]
fn args_reach_the_export_as_json_with_the_graph() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = fixture(&runtime);
    let args = json!({"status": "done", "limit": 2, "all": true});
    let out = run_on(
        &fixture.view(),
        &listing(),
        args.as_object().unwrap(),
        CommandFormat::Json,
        "2026-10-03",
    )
    .unwrap();
    // What the guest decodes: the SDK's CommandInput.
    let input: specforge_extension_sdk::CommandInput =
        serde_json::from_value(last_input(&runtime)).unwrap();
    assert_eq!(Value::Object(input.args), args);
    assert_eq!(input.cwd, canonical_root(&fixture));
    let f1 = input.graph.node("f1").unwrap();
    assert_eq!(f1.title.as_deref(), Some("One"));
    assert!(input.graph.edges().is_empty());
    assert_eq!(
        (out.exit_code, out.stdout.as_str(), out.stderr.as_str()),
        (3, "out\n", "err\n")
    );
}

#[test]
fn run_sends_todays_utc_date() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = fixture(&runtime);
    run(
        &fixture.view(),
        &routed("ok"),
        &Map::new(),
        CommandFormat::Json,
    )
    .unwrap();

    let today = last_input(&runtime)["today"].as_str().unwrap().to_string();
    let parsed = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .unwrap_or_else(|e| panic!("{today:?} is no %Y-%m-%d date: {e}"));
    assert_eq!(parsed, chrono::Utc::now().date_naive());
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the CommandInput carries the format the caller asked for and the host's date"
)]
fn the_input_carries_the_format_and_the_date() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = fixture(&runtime);
    for format in CommandFormat::ALL {
        run_on(
            &fixture.view(),
            &routed("ok"),
            &Map::new(),
            format,
            "2026-10-03",
        )
        .unwrap();
        let input = last_input(&runtime);
        assert_eq!(input["format"], format.as_str());
        assert_eq!(input["today"], "2026-10-03");
        assert_eq!(input["args"], json!({}), "the format is not an arg");
    }
    assert_eq!(CommandFormat::parse("json"), Some(CommandFormat::Json));
    assert_eq!(CommandFormat::parse("table"), None);
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "one operation runs a command over the project view: it normalizes the args, attaches the evidence, the date and the project root, absolute and canonical, and calls the export"
)]
fn one_operation_runs_a_command_over_the_view() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = fixture(&runtime);
    std::fs::write(
        fixture.dir.path().join("specforge-report.json"),
        r#"{"runner":"fixture","results":{}}"#,
    )
    .unwrap();

    run_on(
        &fixture.view(),
        &ordered_ok(),
        json!({"milestone": "m1"}).as_object().unwrap(),
        CommandFormat::Json,
        "2026-10-03",
    )
    .unwrap();

    let input = last_input(&runtime);
    assert_eq!(
        input["args"],
        json!({"order": "desc", "all": false, "limit": 10, "milestone": "m1"})
    );
    assert_eq!(input["cwd"], canonical_root(&fixture));
    assert_eq!(input["format"], "json");
    assert_eq!(input["today"], "2026-10-03");
    assert_eq!(
        input["evidence"],
        json!({"state": "recorded", "entities": {}})
    );
}

#[test]
fn a_refused_arg_runs_nothing() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = fixture(&runtime);
    let err = run(
        &fixture.view(),
        &ordered_ok(),
        json!({"milestone": "m1", "limit": -1}).as_object().unwrap(),
        CommandFormat::Json,
    )
    .unwrap_err();

    let RunError::Args(refused) = err else {
        panic!("expected Args, got {err:?}");
    };
    assert_eq!(
        refused.message(),
        "limit must be a non-negative integer, got -1"
    );
    assert!(runtime.calls().is_empty());
}

#[test]
fn a_rootless_view_runs_nothing() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = fixture(&runtime);
    let err = run(
        &fixture.rootless_view(),
        &routed("ok"),
        &Map::new(),
        CommandFormat::Json,
    )
    .unwrap_err();

    let RunError::NoProject(refused) = err else {
        panic!("expected NoProject, got {err:?}");
    };
    assert_eq!(refused.code, "no_project");
    assert!(runtime.calls().is_empty());
}

#[cfg(unix)]
#[test]
fn the_root_a_command_receives_is_canonical() {
    let runtime = std::sync::Arc::new(fake());
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("real")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
    let fixture = fixture(&runtime);
    let recorded =
        specforge_project::coverage::RecordedCoverage::over(&fixture.graph, fixture.env());
    let linked = dir.path().join("link");
    let view = ProjectView::new(&fixture.graph, fixture.env(), Some(&linked), &recorded);

    run(&view, &routed("ok"), &Map::new(), CommandFormat::Json).unwrap();

    assert_eq!(
        last_input(&runtime)["cwd"],
        std::fs::canonicalize(dir.path().join("real"))
            .unwrap()
            .display()
            .to_string()
    );
}

#[test]
fn a_failure_is_the_error_object_a_command_writes() {
    let missing = RunError::Args(ArgError::Missing {
        names: vec!["milestone".into()],
    });
    assert_eq!(
        serde_json::to_value(missing.to_command_error()).unwrap(),
        json!({"code": "INVALID_INPUT", "message": "missing required arg 'milestone'"})
    );

    let trap = RunError::Call(CallError::new(
        Operation::Command,
        EXT,
        "cmd__x",
        CallFailure::Trapped {
            kind: "k".into(),
            message: "m".into(),
        },
    ));
    assert_eq!(
        serde_json::to_value(trap.to_command_error()).unwrap(),
        json!({"code": "E028",
               "message": "command cmd__x() of '@acme/x' trapped: k: m",
               "suggestion": "report the failure to the author of '@acme/x', or check it is installed and up to date"})
    );

    let rootless = RunError::NoProject(OpError::new(
        crate::OpErrorKind::PreconditionFailed,
        "no_project",
        "this operation needs the project on disk, and this project has none",
    ));
    assert_eq!(
        serde_json::to_value(rootless.to_command_error()).unwrap(),
        json!({"code": "no_project",
               "message": "this operation needs the project on disk, and this project has none"})
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "Wasm trap caught and reported as ExtensionError"
)]
fn a_trapping_command_is_an_extension_error() {
    let fixture = fixture(&std::sync::Arc::new(fake()));
    let err = run(
        &fixture.view(),
        &routed("boom"),
        &Map::new(),
        CommandFormat::Json,
    )
    .unwrap_err();
    let RunError::Call(err) = err else {
        panic!("expected Call, got {err:?}");
    };
    let diagnostic = err.diagnostic();
    assert_eq!(diagnostic.code, "E028");
    assert_eq!(
        diagnostic.message,
        "command cmd__boom() of '@acme/x' trapped: call_failed: unreachable: the command panicked"
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "a command whose output is not a CommandOutput is an ExtensionError, not exit 0 with the raw bytes"
)]
fn a_command_whose_output_is_not_a_command_output_is_an_extension_error() {
    for raw in [
        &b"not json at all"[..],
        br#"{"exit_code":"3","stdout":"x"}"#,
        br#"{}"#,
        br#"[1,2]"#,
    ] {
        let fixture = {
            let mut fixture = crate::view::testing::Fixture::new().runtime(std::sync::Arc::new(
                fake().answer_raw(EXT, "cmd__ok", WasmCallResult::Ok(raw.to_vec())),
            ));
            fixture.graph = graph();
            fixture
        };
        let err = run(
            &fixture.view(),
            &routed("ok"),
            &Map::new(),
            CommandFormat::Json,
        )
        .unwrap_err();
        let RunError::Call(err) = err else {
            panic!("expected Call, got {err:?}");
        };
        let shown = String::from_utf8_lossy(raw);
        assert_eq!(err.operation, Operation::Command, "{shown}");
        assert!(
            matches!(
                err.failure,
                CallFailure::Malformed {
                    expected: "CommandOutput",
                    ..
                }
            ),
            "{shown}: {err}"
        );
        let diagnostic = err.diagnostic();
        assert_eq!(diagnostic.code, "E028");
        assert!(
            diagnostic.message.starts_with(
                "command cmd__ok() of '@acme/x' answered output that is not a CommandOutput: "
            ),
            "{}",
            diagnostic.message
        );
    }
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "the command runs in the runtime that read the project's declarations, which loaded only the extensions the project enables"
)]
fn a_command_runs_in_the_runtime_that_read_the_declarations() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({"name": "p", "version": "0.1.0", "extensions": ["@specforge/product"]}).to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.spec"),
        "feature f1 \"One\" {\n  status done\n  problem \"p\"\n}\n",
    )
    .unwrap();

    // What the CLI does: one runtime, the project compiled through it,
    // then the routed command run in it.
    let runtime = std::sync::Arc::new(specforge_component::ComponentRuntime::with_user_cache());
    assert!(runtime.loaded_names().is_empty(), "a runtime starts empty");
    let project = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    let loaded = runtime.loaded_names();
    assert_eq!(
        loaded,
        ["@specforge/product"],
        "only what the project enables"
    );
    assert!(
        project
            .environment()
            .enabled()
            .iter()
            .all(|e| e.failure.is_none())
    );
    let commands = ExtensionCommands::build(project.environment().registries());
    let features = commands
        .all()
        .iter()
        .find(|c| c.id() == "features")
        .expect("product declares `features`");

    let out = run(
        &ProjectView::of(&project),
        features,
        &Map::new(),
        CommandFormat::Json,
    )
    .unwrap();
    assert_eq!(out.exit_code, 0, "{}", out.stderr);
    let listed: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(listed["features"][0]["id"], "f1", "{listed}");
    assert_eq!(
        runtime.loaded_names(),
        loaded,
        "running it loaded no module"
    );
}

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "a declared export the guest does not route is an ExtensionError when dispatched"
)]
fn an_export_the_guest_does_not_route_is_an_extension_error() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        json!({"name": "p", "version": "0.1.0", "extensions": ["@specforge/product"]}).to_string(),
    )
    .unwrap();
    let runtime = std::sync::Arc::new(specforge_component::ComponentRuntime::new());
    let project = specforge_project::CompiledProject::compile(
        &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        Some(runtime.clone()),
    );
    let unrouted = CommandDescriptor {
        export: "cmd__product_no_such_command".into(),
        ..command("no_such_command")
    };
    let err = run(
        &ProjectView::of(&project),
        &ExtensionCommand::new("@specforge/product", "product", &unrouted),
        &Map::new(),
        CommandFormat::Json,
    )
    .unwrap_err();
    let RunError::Call(err) = err else {
        panic!("expected Call, got {err:?}");
    };
    assert_eq!(err.diagnostic().code, "E028");
    assert_eq!(
        err.to_string(),
        "command cmd__product_no_such_command() of '@specforge/product' trapped: guest_error: \
             unknown export 'cmd__product_no_such_command'"
    );
}

#[specforge_test(
    behavior = "call_extension_exports",
    verify = "every extension call encodes its input as the protocol type the SDK decodes"
)]
fn the_command_input_is_the_wire_golden() {
    let runtime = std::sync::Arc::new(fake());
    let fixture = crate::view::testing::Fixture::new().runtime(runtime.clone());
    let env = fixture.env();
    let graph = graph();
    let recorded = specforge_project::coverage::RecordedCoverage::over(&graph, env);
    let view = ProjectView::new(&graph, env, Some(Path::new("/p")), &recorded);
    let args = json!({"status": "done", "limit": 2, "all": true});
    run_on(
        &view,
        &listing(),
        args.as_object().unwrap(),
        CommandFormat::Json,
        "2026-10-03",
    )
    .unwrap();

    let path =
        specforge_repo_root::repo_root().join("crates/runtime/wasm/tests/wire/command.input.json");
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(last_input(&runtime), expected, "golden command.input.json");
}
