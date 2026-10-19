use super::*;

#[specforge_test(
    behavior = "management_operations_over_the_project_view",
    verify = "add, update and remove refuse an unusable specforge.json with one refusal, before they write"
)]
fn an_unusable_config_is_refused_before_anything_is_installed() {
    use crate::config::testing::{UNUSABLE, files_under};

    for config in UNUSABLE {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
        let before = files_under(dir.path());
        let read = specforge_common::read_project_config(dir.path());
        let refused = crate::config::refusal(&read.problems[0]);

        for source in [
            Source::Builtin("@specforge/product"),
            // A file that is not there: the config is refused first.
            Source::Local(dir.path().join("missing.wasm")),
            Source::Registry(PackageRef::parse("@acme/tool").unwrap()),
        ] {
            for dry_run in [false, true] {
                let request = AddRequest {
                    root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                    extensions: vec![source.clone().into()],
                    base: None,
                    allow_unsigned: false,
                    dry_run,
                    caller: crate::testing::caller(),
                };
                let error =
                    add(&request, &crate::registry::Unconfigured("add"), &runtime()).unwrap_err();

                assert_eq!(error.error, refused, "{config}: {source:?}");
                assert_eq!(files_under(dir.path()), before, "{config}: {source:?}");
            }
        }
    }
}

#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "an install of an extension whose binary changed after install replaces it"
)]
fn add_reinstalls_a_binary_that_changed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("specforge.json"),
        r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
    )
    .unwrap();
    let files = tempfile::tempdir().unwrap();
    let greet = files.path().join("greet.wasm");
    std::fs::write(&greet, GREET).unwrap();
    let request = AddRequest {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        extensions: vec![Source::Local(greet).into()],
        base: None,
        allow_unsigned: false,
        dry_run: false,
        caller: crate::testing::caller(),
    };
    let unconfigured = crate::registry::Unconfigured("add");
    add(&request, &unconfigured, &runtime()).unwrap();
    let module = dir
        .path()
        .join(".specforge/extensions/@sdk/greet/extension.wasm");
    let pinned = std::fs::read(&module).unwrap();
    let mut changed = pinned.clone();
    changed.extend_from_slice(b"changed after install");
    std::fs::write(&module, &changed).unwrap();

    let added = add(&request, &unconfigured, &runtime()).unwrap();

    assert!(
        matches!(added.added[0].outcome, AddOutcome::Installed { .. }),
        "{:?}",
        added.added[0].outcome
    );
    assert_eq!(std::fs::read(&module).unwrap(), pinned);
    assert_eq!(added.writes.len(), 1, "only the module differed");

    // Now that it is the pinned binary again, adding it is a no-op.
    let again = add(&request, &unconfigured, &runtime()).unwrap();
    assert!(matches!(
        again.added[0].outcome,
        AddOutcome::AlreadyPresent { .. }
    ));
}

#[test]
fn add_refuses_a_binary_that_claims_a_builtins_name() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let file = tempfile::tempdir().unwrap();
    let wasm = file.path().join("product.wasm");
    std::fs::write(&wasm, b"\0asm impostor").unwrap();
    let impostor = runtime().binary(
        b"\0asm impostor",
        declaring("@specforge/product", "9.9.9", &[]),
    );
    let before = files_under(dir.path());

    let error = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &wasm,
        ),
        &crate::registry::Unconfigured("add"),
        &impostor,
    )
    .unwrap_err();

    assert_eq!(error.error.code, "extension_conflict");
    assert_eq!(error.error.kind, OpErrorKind::Conflict);
    assert_eq!(
        error.error.message,
        "the extension declares the name of the builtin '@specforge/product'"
    );
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("enable the builtin instead: specforge add @specforge/product")
    );
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add unresolvable extension rejects with diagnostic"
)]
fn a_local_file_add_cannot_read_is_e054() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let registry = crate::registry::Unconfigured("add");

    let before = files_under(dir.path());
    let missing = dir.path().join("missing.wasm");
    let error = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &missing,
        ),
        &registry,
        &runtime(),
    )
    .unwrap_err();
    assert_eq!(error.error.code, "E054");
    assert_eq!(
        error.error.message,
        format!("file not found: {}", missing.display())
    );
    assert_eq!(files_under(dir.path()), before);
    assert_eq!(files_under(dir.path()), before);
}

#[test]
fn an_empty_directory_is_no_extension_crate() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let before = files_under(dir.path());
    let scratch = tempfile::tempdir().unwrap();
    let directory = scratch.path().join("a-directory");
    std::fs::create_dir(&directory).unwrap();

    let error = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &directory,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap_err();

    assert_eq!(error.error.code, "E040");
    assert!(
        error.error.message.starts_with("no built component in"),
        "{}",
        error.error.message
    );
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a local crate directory installs the component its crate builds"
)]
fn a_crate_directory_installs_its_built_component() {
    let dir = project_with(EMPTY_PROJECT);
    let scratch = tempfile::tempdir().unwrap();
    let krate = scratch.path().join("greet-ext");
    let release = krate.join("target/wasm32-wasip2/release");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"greet-ext\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let component = release.join("greet_ext.wasm");
    std::fs::write(&component, GREET).unwrap();

    let added = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &krate,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap();

    assert!(
        matches!(added.added[0].outcome, AddOutcome::Installed { .. }),
        "{:?}",
        added.added[0].outcome
    );
    let lock = std::fs::read_to_string(dir.path().join("specforge.lock")).unwrap();
    assert!(
        lock.contains("local:")
            && lock.contains("greet-ext/target/wasm32-wasip2/release/greet_ext.wasm\""),
        "{lock}"
    );
}

#[test]
fn a_local_file_that_is_no_extension_is_e028() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let file = tempfile::tempdir().unwrap();
    let wasm = file.path().join("junk.wasm");
    std::fs::write(&wasm, b"not wasm").unwrap();
    let before = files_under(dir.path());

    let error = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &wasm,
        ),
        &crate::registry::Unconfigured("add"),
        &InProcessRuntime::new(),
    )
    .unwrap_err();

    assert_eq!(error.error.code, "E028");
    assert!(
        error
            .error
            .message
            .starts_with("not a loadable SpecForge extension:"),
        "{}",
        error.error.message
    );
    assert!(
        error
            .error
            .suggestion
            .as_deref()
            .unwrap()
            .contains("specforge-extension-sdk")
    );
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "an install over an unreadable specforge.lock is refused before anything is written"
)]
fn an_unreadable_lock_refuses_before_the_local_file_is_read() {
    let dir = project_with(EMPTY_PROJECT);
    std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();
    let missing = dir.path().join("missing.wasm");

    let error = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &missing,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap_err();

    assert_eq!(error.error.code, "E033");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("specforge.lock")).unwrap(),
        "not a lock {{{"
    );
}

#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "add, init and publish read a candidate's declaration in the runtime their surface passes"
)]
fn a_candidate_is_read_in_the_runtime_the_caller_passes() {
    let dir = project_with(EMPTY_PROJECT);
    let files = tempfile::tempdir().unwrap();
    let greet = files.path().join("greet.wasm");
    std::fs::write(&greet, GREET).unwrap();
    let runtime = runtime();

    add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &greet,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime,
    )
    .unwrap();

    let handshakes: Vec<_> = runtime
        .calls()
        .into_iter()
        .filter(|call| call.extension == "__candidate" && call.export == "__handshake")
        .collect();
    assert_eq!(handshakes.len(), 1, "{handshakes:?}");
    assert!(
        !specforge_wasm::WasmRuntime::unload(&runtime, "__candidate"),
        "the candidate is left unloaded"
    );
}

#[test]
fn add_reads_specforge_json_once_before_its_edit() {
    let dir = project_with(
        r#"{"name": "p", "version": "0.1.0", "sentinel": {"keep": [1, 2]}, "extensions": []}"#,
    );
    let files = tempfile::tempdir().unwrap();
    let greet = files.path().join("greet.wasm");
    std::fs::write(&greet, GREET).unwrap();

    let added = add(
        &local_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            &greet,
        ),
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap();

    assert!(matches!(
        added.added[0].outcome,
        AddOutcome::Installed { .. }
    ));
    assert_eq!(added.extensions_enabled, 1);
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("specforge.json")).unwrap())
            .unwrap();
    assert_eq!(config["sentinel"], serde_json::json!({"keep": [1, 2]}));
    assert_eq!(config["extensions"], serde_json::json!(["@sdk/greet"]));
}

#[test]
fn a_missing_config_is_config_not_found_with_the_hint_to_init() {
    let dir = tempfile::tempdir().unwrap();
    let request = AddRequest {
        root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
        extensions: vec![Source::Builtin("@specforge/product").into()],
        base: None,
        allow_unsigned: false,
        dry_run: false,
        caller: crate::testing::caller(),
    };

    let error = add(&request, &crate::registry::Unconfigured("add"), &runtime()).unwrap_err();

    assert_eq!(error.error.code, "config_not_found");
    assert!(error.error.suggestion.unwrap().contains("specforge init"));
    assert!(!dir.path().join("specforge.json").exists());
}

#[cfg(unix)]
#[test]
fn an_add_writes_nothing_outside_its_project_whatever_bound_its_caller_lends() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let (_files, wasm) = greet_file();
    // The lock lives in a sibling directory the project links to.
    let sibling = tempfile::tempdir().unwrap();
    let target = sibling.path().join("specforge.lock");
    std::fs::write(&target, r#"{"lockfile_version": 1, "entries": []}"#).unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("specforge.lock")).unwrap();
    let (lock_before, files_before) = (std::fs::read(&target).unwrap(), files_under(dir.path()));
    // A bound wider than the project, holding the sibling too.
    let wide = crate::Containment::within(&std::env::temp_dir());

    let error = add(
        &AddRequest {
            caller: crate::testing::bounded(&wide),
            ..local_request(
                &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                &wasm,
            )
        },
        &crate::registry::Unconfigured("add"),
        &runtime(),
    )
    .unwrap_err();

    assert_eq!(error.error.code, crate::OUTSIDE_ROOT, "{error:?}");
    assert_eq!(std::fs::read(&target).unwrap(), lock_before);
    assert_eq!(files_under(dir.path()), files_before);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "an add whose installs are all present writes only specforge.json, wherever its lock links"
)]
fn an_add_whose_installs_are_all_present_writes_no_lock_and_is_not_bounded_by_it() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let (_files, wasm) = greet_file();
    let root = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let runtime = serving_builtin(
        runtime(),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    );
    // Installed and enabled first, by an unbounded caller.
    add(
        &local_request(&root, &wasm),
        &crate::registry::Unconfigured("add"),
        &runtime,
    )
    .unwrap();
    // Then the lock is moved to a sibling and linked back: reading it is fine, writing it would leave the project.
    let sibling = tempfile::tempdir().unwrap();
    let target = sibling.path().join("specforge.lock");
    std::fs::rename(dir.path().join("specforge.lock"), &target).unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("specforge.lock")).unwrap();
    let (lock_before, files_before) = (std::fs::read(&target).unwrap(), files_under(dir.path()));
    let wide = crate::Containment::within(&std::env::temp_dir());

    let added = add(
        &AddRequest {
            extensions: vec![
                Source::Local(wasm.clone()).into(),
                Source::Builtin("@specforge/product").into(),
            ],
            caller: crate::testing::bounded(&wide),
            ..local_request(&root, &wasm)
        },
        &crate::registry::Unconfigured("add"),
        &runtime,
    )
    .unwrap();

    assert!(
        matches!(added.added[0].outcome, AddOutcome::AlreadyPresent { .. }),
        "{added:?}"
    );
    assert!(
        matches!(
            added.added[1].outcome,
            AddOutcome::Builtin { changed: true, .. }
        ),
        "{added:?}"
    );
    assert_eq!(added.writes.named(Some(dir.path())), ["specforge.json"]);
    assert_eq!(std::fs::read(&target).unwrap(), lock_before);
    // Only specforge.json changed in the project.
    let after = files_under(dir.path());
    assert_eq!(after.len(), files_before.len());
    assert!(after.iter().all(|(path, bytes)| {
        path.ends_with("specforge.json") || files_before.get(path) == Some(bytes)
    }));
}
