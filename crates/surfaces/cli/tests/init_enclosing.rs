use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;
use std::fs;

#[specforge_test(
    behavior = "scaffold_new_project",
    verify = "init in a directory whose files the enclosing project reads is refused, and that project keeps them"
)]
fn init_in_a_directory_the_enclosing_project_reads_is_refused_writing_nothing() {
    let dir = TestProject::new()
        .file("a.spec", "spec \"t\" {\n  version \"0.1.0\"\n}\n")
        .file("sub/t.spec", "// t\n");
    let sub = dir.root().join("sub");

    Cli::on(&dir)
        .bare_command(&["init", "--name", "nested"])
        .current_dir(&sub)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("error[inside_project_sources]"))
        .stderr(predicates::str::contains("t.spec"));

    assert_eq!(fs::read_dir(&sub).unwrap().count(), 1);
    Cli::on(&dir).bare_command(&["check"]).assert().success();
}
