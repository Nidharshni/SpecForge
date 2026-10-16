use super::*;

fn builtin_request<'a>(
    root: &'a specforge_common::ProjectRoot,
    name: &'static str,
) -> AddRequest<'a> {
    AddRequest {
        root,
        extensions: vec![Source::Builtin(name).into()],
        allow_unsigned: false,
        trust: Trust::Refuse,
        dry_run: false,
        containment: &UNBOUNDED,
    }
}

/// `runtime()`, also serving the builtins `@specforge/cargo-test` (which
/// requires `@specforge/testing`) and `@specforge/testing`.
fn runtime_with_cargo_test() -> InProcessRuntime {
    serving_builtin(
        serving_builtin(
            runtime(),
            "@specforge/cargo-test",
            declaring(
                "@specforge/cargo-test",
                "1.0.0",
                &[("@specforge/testing", "^1.0", false)],
            ),
        ),
        "@specforge/testing",
        declaring("@specforge/testing", "1.0.0", &[]),
    )
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add enables a builtin's required peers but not its optional ones"
)]
fn a_builtin_is_enabled_after_its_required_builtin_peers() {
    let dir = project_with(EMPTY_PROJECT);

    let added = add(
        &builtin_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            "@specforge/cargo-test",
        ),
        &crate::registry::Unconfigured("add"),
        &runtime_with_cargo_test(),
    )
    .unwrap();

    assert_eq!(
        added.added[0].outcome,
        AddOutcome::Builtin {
            name: "@specforge/cargo-test",
            changed: true,
            peers_enabled: vec!["@specforge/testing"],
        }
    );
    assert_eq!(added.writes.named(Some(dir.path())), ["specforge.json"]);
    assert_eq!(
        specforge_common::read_project_config(dir.path())
            .config
            .extensions,
        ["@specforge/testing", "@specforge/cargo-test"]
    );
    assert_eq!(added.extensions_enabled, 2);
}

#[test]
fn extensions_enabled_counts_the_string_entries_after_the_add() {
    let dir = project_with(r#"{"name":"p","version":"0.1.0","extensions":[1,"@specforge/rust"]}"#);
    let registry = crate::registry::Unconfigured("add");
    let project = specforge_common::ProjectRoot::of(dir.path()).unwrap();
    let request = builtin_request(&project, "@specforge/product");
    let runtime = serving_builtin(
        serving_builtin(
            runtime(),
            "@specforge/product",
            declaring("@specforge/product", "1.0.0", &[]),
        ),
        "@specforge/rust",
        declaring("@specforge/rust", "1.0.0", &[]),
    );

    let added = add(&request, &registry, &runtime).unwrap();
    assert_eq!(added.extensions_enabled, 2);

    let again = add(&request, &registry, &runtime).unwrap();
    assert!(matches!(
        again.added[0].outcome,
        AddOutcome::Builtin { changed: false, .. }
    ));
    assert!(again.writes.is_empty());
    assert_eq!(again.extensions_enabled, 2);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a builtin whose declaration cannot be read is refused before anything is written"
)]
fn a_builtin_whose_declaration_cannot_be_read_is_refused() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let before = files_under(dir.path());

    let error = add(
        &builtin_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            "@specforge/cargo-test",
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
            .starts_with("the builtin '@specforge/cargo-test' does not load: "),
        "{}",
        error.error.message
    );
    assert_eq!(files_under(dir.path()), before);
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "add enables the builtins a required builtin peer requires, dependencies first"
)]
fn add_enables_what_a_required_builtin_requires_first() {
    let dir = project_with(EMPTY_PROJECT);
    let registry = crate::registry::Unconfigured("add");
    let chain = serving_builtin(
        serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/formal",
                declaring(
                    "@specforge/formal",
                    "1.0.0",
                    &[("@specforge/software", "^1.0", false)],
                ),
            ),
            "@specforge/software",
            declaring(
                "@specforge/software",
                "1.0.0",
                &[("@specforge/product", "^1.0", false)],
            ),
        ),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    );

    let added = add(
        &builtin_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            "@specforge/formal",
        ),
        &registry,
        &chain,
    )
    .unwrap();

    assert_eq!(
        added.added[0].outcome,
        AddOutcome::Builtin {
            name: "@specforge/formal",
            changed: true,
            peers_enabled: vec!["@specforge/product", "@specforge/software"],
        }
    );
    assert_eq!(
        specforge_common::read_project_config(dir.path())
            .config
            .extensions,
        [
            "@specforge/product",
            "@specforge/software",
            "@specforge/formal"
        ]
    );
    assert_eq!(added.writes.named(Some(dir.path())), ["specforge.json"]);

    // A cycle among required builtins stops the walk: the registry build
    // reports it, and the add succeeds.
    let cyclic_dir = project_with(EMPTY_PROJECT);
    let cycle = serving_builtin(
        serving_builtin(
            runtime(),
            "@specforge/formal",
            declaring(
                "@specforge/formal",
                "1.0.0",
                &[("@specforge/software", "^1.0", false)],
            ),
        ),
        "@specforge/software",
        declaring(
            "@specforge/software",
            "1.0.0",
            &[("@specforge/formal", "^1.0", false)],
        ),
    );
    let added = add(
        &builtin_request(
            &specforge_common::ProjectRoot::of(cyclic_dir.path()).unwrap(),
            "@specforge/formal",
        ),
        &registry,
        &cycle,
    )
    .unwrap();
    assert!(matches!(
        added.added[0].outcome,
        AddOutcome::Builtin { changed: true, .. }
    ));
}

#[specforge_test(
    behavior = "add_extension_to_existing_project",
    verify = "a required builtin peer the embedded builtin does not satisfy is refused before anything is written"
)]
fn a_required_builtin_peer_the_embedded_one_does_not_satisfy_is_refused() {
    use crate::config::testing::files_under;
    let dir = project_with(EMPTY_PROJECT);
    let before = files_under(dir.path());
    // formal wants software ^2.0; the software this specforge embeds is 1.0.0. (Init also
    // enables testing beside software.)
    let runtime = serving_builtin(
        serving_builtin(
            serving_builtin(
                runtime(),
                "@specforge/formal",
                declaring(
                    "@specforge/formal",
                    "1.0.0",
                    &[("@specforge/software", "^2.0", false)],
                ),
            ),
            "@specforge/software",
            declaring("@specforge/software", "1.0.0", &[]),
        ),
        "@specforge/testing",
        declaring("@specforge/testing", "1.0.0", &[]),
    );

    let error = add(
        &builtin_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            "@specforge/formal",
        ),
        &crate::registry::Unconfigured("add"),
        &runtime,
    )
    .unwrap_err();

    assert_eq!(error.error.code, "E027");
    assert_eq!(
        error.error.message,
        "extension '@specforge/formal' requires peer dependency '@specforge/software' ^2.0 but version 1.0.0 is installed"
    );
    assert_eq!(
        error.error.suggestion.as_deref(),
        Some("this specforge build's builtin extensions disagree: reinstall specforge")
    );
    assert_eq!(files_under(dir.path()), before);
}

#[cfg(unix)]
#[specforge_test(
    behavior = "install_wasm_extension",
    verify = "a failed install puts back the binary, specforge.lock and specforge.json it changed"
)]
fn a_failed_builtin_edit_enables_no_peer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project_with(EMPTY_PROJECT);
    let config = dir.path().join("specforge.json");
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = std::fs::read(&config).unwrap();

    let error = add(
        &builtin_request(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            "@specforge/cargo-test",
        ),
        &crate::registry::Unconfigured("add"),
        &runtime_with_cargo_test(),
    )
    .unwrap_err();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(error.error.code, "config_write_failed", "{error:?}");
    assert!(error.error.writes.is_empty());
    assert_eq!(std::fs::read(&config).unwrap(), before);
}
