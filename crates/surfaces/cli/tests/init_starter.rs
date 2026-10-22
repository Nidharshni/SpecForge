//! A project's starter is checked by the extensions `init` enables (ADR 0189).

use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "a directory named after a kind init enables is refused as the project name, writing nothing"
)]
fn a_directory_named_after_a_kind_init_enables_is_refused_writing_nothing() {
    let dir = TestProject::new().without_config();
    let release = dir.root().join("release");
    std::fs::create_dir_all(&release).unwrap();

    Cli::on(&dir)
        .bare_command(&[
            "init",
            "--extensions",
            "@specforge/product,@specforge/software",
        ])
        .current_dir(&release)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("invalid project name 'release'"));
    assert_eq!(std::fs::read_dir(&release).unwrap().count(), 0);
}

#[specforge_test(
    behavior = "non_interactive_init",
    verify = "an init whose version holds a quote or a backslash writes a starter that passes format --check and check"
)]
fn an_init_whose_version_holds_a_quote_formats_and_checks_clean() {
    let dir = TestProject::new().without_config();
    Cli::on(&dir)
        .bare_command(&[])
        .args([
            "init",
            "--name",
            "quoted",
            "--version",
            "1.0\" oops {\\q",
            "--extensions",
            "@specforge/software",
        ])
        .current_dir(dir.root())
        .assert()
        .success();
    for command in [&["format", "--check"][..], &["check"][..]] {
        let run = Cli::on(&dir)
            .bare_command(&[])
            .args(command)
            .current_dir(dir.root())
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{command:?}:\n{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
        );
    }
}
