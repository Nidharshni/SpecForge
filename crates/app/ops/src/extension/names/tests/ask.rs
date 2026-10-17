//! `Names::ask`: the specifiers of a change asked of its opened lock plan (ADR 0134).

use super::*;
use crate::extension::lock_plan::{Asked, LockPlan};
use crate::testing::{declaring, serving_builtin};
use specforge_installed::Installed;

fn init_plan<'a>(
    installed: &'a Installed,
    runtime: &'a InProcessRuntime,
    names: &Names<'_>,
) -> LockPlan<'a> {
    LockPlan::initializing(
        installed,
        runtime,
        crate::testing::caller(),
        names.installs(),
        serde_json::json!({"name": "p"}),
    )
    .unwrap()
}

fn product_runtime() -> InProcessRuntime {
    serving_builtin(
        testing::candidates(),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    )
}

#[test]
fn each_specifier_is_asked_under_its_position() {
    let dir = tempfile::tempdir().unwrap();
    let greet = dir.path().join("greet.wasm");
    write_greet(&greet);
    let strings = [
        "@specforge/product".to_string(),
        greet.display().to_string(),
        "@specforge/product@latest".to_string(),
    ];
    let names = Names::read(&strings, place(dir.path(), &UNBOUNDED)).unwrap();
    let (runtime, installed) = (product_runtime(), Installed::at(dir.path()));

    let (checked, answers) = names
        .ask(
            init_plan(&installed, &runtime, &names),
            &runtime,
            &mut no_packages,
            &mut |install| install.name().to_string(),
        )
        .unwrap();

    assert_eq!(
        answers,
        [
            Answer::Builtin("@specforge/product"),
            Answer::Installed {
                recorded: "@sdk/greet".to_string(),
                starter: None
            },
            Answer::Duplicate {
                name: "@specforge/product".to_string(),
                first: "@specforge/product".to_string()
            },
        ]
    );
    assert_eq!(checked.entries(), ["@specforge/product", "@sdk/greet"]);
    let done = checked.finish().unwrap();
    assert!(matches!(done.asked(Ask(0)), Asked::Changes { .. }));
    assert!(matches!(done.asked(Ask(1)), Asked::Changes { .. }));
    // The duplicate asked nothing, so its ask was never answered.
    let unanswered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| done.asked(Ask(2))));
    assert!(unanswered.is_err());
}

#[test]
fn a_plan_refusal_is_named_by_the_specifier_it_is_charged_to() {
    let dir = tempfile::tempdir().unwrap();
    let needs = dir.path().join("needs.wasm");
    std::fs::write(&needs, testing::NEEDS_SOFTWARE_9).unwrap();
    let shown = needs.display().to_string();
    let (runtime, installed) = (testing::probes(), Installed::at(dir.path()));
    let refusal = "extension '@probe/needs-software' requires peer dependency '@specforge/software' ^9.0 but version 1.0.0 is installed";

    let refused = |strings: &[String]| {
        let names = Names::read(strings, place(dir.path(), &UNBOUNDED)).unwrap();
        names
            .ask(
                init_plan(&installed, &runtime, &names),
                &runtime,
                &mut no_packages,
                &mut |_| (),
            )
            .err()
            .unwrap()
    };

    let several = refused(&[shown.clone(), "@specforge/software".to_string()]);
    assert_eq!(several.code, "E027", "{several:?}");
    assert_eq!(several.message, format!("adding '{shown}': {refusal}"));
    let alone = refused(&[shown]);
    assert_eq!(alone.message, refusal);
}

#[test]
fn a_package_a_plan_cannot_fetch_is_refused_named() {
    let dir = tempfile::tempdir().unwrap();
    let strings = ["@specforge/product".to_string(), "@acme/x".to_string()];
    let names = Names::read(&strings, place(dir.path(), &UNBOUNDED)).unwrap();
    let (runtime, installed) = (product_runtime(), Installed::at(dir.path()));

    let error = names
        .ask(
            init_plan(&installed, &runtime, &names),
            &runtime,
            &mut |_, _| Ok(Version::parse("1.0.0").unwrap()),
            &mut |_| (),
        )
        .err()
        .unwrap();

    assert_eq!(error.code, "E063", "{error:?}");
    assert!(
        error
            .message
            .starts_with("adding '@acme/x': no registry configured: `init` needs one"),
        "{error:?}"
    );
}
