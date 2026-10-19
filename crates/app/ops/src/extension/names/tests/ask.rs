//! `Names::ask`: the specifiers of a change asked of its opened lock plan (ADR 0134).

use super::*;
use crate::extension::lock_plan::{Asked, LockPlan};
use crate::testing::{declaring, serving_builtin};
use specforge_installed::Installed;

fn init_plan<'a>(installed: &'a Installed, runtime: &'a InProcessRuntime) -> LockPlan<'a> {
    LockPlan::initializing(
        installed,
        runtime,
        crate::testing::caller(),
        serde_json::json!({"name": "p"}),
    )
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
            init_plan(&installed, &runtime),
            &runtime,
            &mut no_packages,
            &mut |install| install.name().to_string(),
        )
        .unwrap();

    assert_eq!(
        answers,
        [
            Answer::Builtin {
                name: "@specforge/product",
                asked: Asked::Changes
            },
            Answer::Installed {
                recorded: "@sdk/greet".to_string(),
                asked: Asked::Changes,
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
    assert!(done.peers_enabled(Ask(0)).is_empty());
    assert!(done.peers_enabled(Ask(1)).is_empty());
    assert!(
        done.peers_enabled(Ask(2)).is_empty(),
        "the duplicate asked nothing, so nothing was enabled for it"
    );
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
                init_plan(&installed, &runtime),
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
            init_plan(&installed, &runtime),
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

#[test]
fn a_lock_the_change_cannot_write_refuses_before_any_file_is_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("specforge.lock"), "not a lock {{{").unwrap();
    let missing = dir.path().join("missing.wasm").display().to_string();
    let (runtime, installed) = (product_runtime(), Installed::at(dir.path()));
    let asked = |strings: &[String]| {
        let names = Names::read(strings, place(dir.path(), &UNBOUNDED)).unwrap();
        names
            .ask(
                init_plan(&installed, &runtime),
                &runtime,
                &mut no_packages,
                &mut |_| (),
            )
            .map(drop)
    };

    // Not E054 for the missing file, and not named by a specifier: the lock refuses first.
    let error = asked(&["@specforge/product".to_string(), missing]).unwrap_err();
    assert_eq!(error.code, "E033", "{error:?}");
    assert!(!error.message.starts_with("adding '"), "{error:?}");
    // A change that installs nothing accepts the lock (ADR 0041 B4).
    assert!(asked(&["@specforge/product".to_string()]).is_ok());
}
