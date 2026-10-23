//! `specforge remove` judges dependents on the command line as the operation does: a builtin another enabled
//! extension requires is refused with E027, and `--force` removes it (ADR 0041). Builtins declare peers, so
//! this runs as a binary (the locked requirers run in process: ops `locked_requirers.rs`).

use specforge_test_macros::test as specforge_test;

use crate::support::{Cli, TestProject};

#[specforge_test(
    behavior = "remove_extension",
    verify = "removing an extension another installed extension requires fails with E027 unless --force"
)]
fn remove_refuses_a_builtin_another_requires_with_e027_unless_forced() {
    let dir = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/formal"])
        .file("main.spec", "");
    let config_path = dir.root().join("specforge.json");
    let config = std::fs::read(&config_path).unwrap();

    let human = Cli::on(&dir)
        .run(&["remove", "@specforge/software"])
        .exits(1);
    assert!(
        human
            .stderr
            .contains("error[E027]: removing '@specforge/software' breaks '@specforge/formal'"),
        "{}",
        human.stderr
    );
    let json = Cli::on(&dir)
        .run(&["remove", "@specforge/software", "--format", "json"])
        .exits(1);
    assert!(json.stdout.contains("\"E027\""), "{}", json.stdout);
    assert_eq!(
        std::fs::read(&config_path).unwrap(),
        config,
        "a refusal writes nothing"
    );

    Cli::on(&dir)
        .run(&["remove", "@specforge/software", "--force"])
        .exits(0);
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    assert_eq!(
        after["extensions"],
        serde_json::json!(["@specforge/formal"])
    );
}
