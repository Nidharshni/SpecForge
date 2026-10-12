//! `check-links` on a fixture project: one declared link, one `verify` text
//! the entity does not declare (A016), one link to an entity that is not there.

use std::process::Command;

fn run(root: &str) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_check-links"))
        .arg(root)
        .arg("tests")
        .output()
        .expect("check-links runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn reports_the_undeclared_text_and_the_unknown_entity() {
    let (code, stdout) = run(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/links"));
    assert_eq!(code, 1, "{stdout}");
    assert_eq!(
        stdout.lines().filter(|l| l.starts_with("A016 ")).count(),
        1,
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "A016 tests/links.rs:7 task_id_uniqueness \"concurrent task creation never produces duplicate id\""
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("declared: \"concurrent task creation never produces duplicate ids\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("UNKNOWN tests/links.rs:13 no_such_invariant"),
        "{stdout}"
    );
    assert!(!stdout.contains("links.rs:2 "), "{stdout}");
}
