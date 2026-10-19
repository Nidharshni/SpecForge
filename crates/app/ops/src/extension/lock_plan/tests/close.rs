use super::*;

#[test]
fn a_builtin_that_does_not_load_refuses_the_plan_e028() {
    let installed = nowhere(Vec::new());
    // The runtime serves `APP`, not the builtin `@specforge/software`.
    let runtime = runtime();
    let entries = ["@specforge/software".to_string()];
    let mut plan = LockPlan::adding(
        &installed,
        opened(&entries),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
    );
    stage(&mut plan, local(&runtime, APP, "app.wasm"));

    let err = refused(plan);

    assert_eq!(err.code, "E028", "{err:?}");
    assert!(
        err.message
            .starts_with("the builtin '@specforge/software' does not load"),
        "{err:?}"
    );
}

#[test]
fn close_enables_required_builtins_dependencies_first() {
    const APP_NEEDS_FORMAL: &[u8] = b"\0asm in-process @acme/app 1.0.0 needs formal";
    let builtin = |runtime, name: &'static str, peers| {
        crate::testing::serving_builtin(runtime, name, declaring(name, "1.0.0", peers))
    };
    // formal requires software, which has product as an optional peer.
    let runtime = runtime().binary(
        APP_NEEDS_FORMAL,
        declaring(
            "@acme/app",
            "1.0.0",
            &[("@specforge/formal", "^1.0", false)],
        ),
    );
    let runtime = builtin(
        runtime,
        "@specforge/formal",
        &[("@specforge/software", "^1.0", false)],
    );
    let runtime = builtin(
        runtime,
        "@specforge/software",
        &[("@specforge/product", "^1.0", true)],
    );
    let installed = nowhere(Vec::new());

    let mut plan = LockPlan::adding(
        &installed,
        opened::<&str>(&[]),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
    );
    stage(&mut plan, local(&runtime, APP_NEEDS_FORMAL, "app.wasm"));
    plan.close().unwrap();

    let appended: Vec<(&str, Option<&str>)> = plan
        .appended
        .iter()
        .map(|a| (a.name.as_str(), a.required_by.as_deref()))
        .collect();
    assert_eq!(
        appended,
        [
            ("@specforge/software", Some("@acme/app")),
            ("@specforge/formal", Some("@acme/app")),
            ("@acme/app", None),
        ],
        "the optional product is not enabled"
    );
    plan.close().unwrap();
    assert_eq!(plan.appended.len(), 3, "idempotent");

    // Asked in the wrong order, a builtin still comes after the builtin it requires.
    let mut plan = LockPlan::adding(
        &installed,
        opened::<&str>(&[]),
        &runtime,
        fetching(no_registry()),
        crate::testing::caller(),
        Purpose::Commit,
    );
    plan.enable(Ask(0), "@specforge/formal");
    plan.enable(Ask(0), "@specforge/software");
    plan.close().unwrap();
    let names: Vec<&str> = plan.appended.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["@specforge/software", "@specforge/formal"]);
}

#[test]
fn update_judges_without_enabling() {
    let installed = nowhere(vec![entry("@acme/app", "1.0.0", "registry", &[])]);
    let runtime = runtime();
    let mut plan = plan(&installed, &runtime, Verb::Update);
    stage(&mut plan, local(&runtime, APP_NEEDS_BASE_2, "app.wasm"));

    let err = refused(plan);

    assert_eq!(err.code, "E027", "{err:?}");
    assert_eq!(
        err.message,
        "extension '@acme/app' requires peer dependency '@acme/base' ^2.0 which is not installed"
    );
}
