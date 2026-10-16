//! What the CLI prints for the refusals of analyze and trace, pinned once before any of them moves (round 13,
//! plan 10). ADR 0029 D9a: the human rendering and the streams are the binary contract, so each run is printed
//! whole (`$ specforge …`, the exit code, both streams), the project root redacted.

use crate::support::{Cli, TestProject};

const BEHAVIORS: &str = "behavior login \"Log in\" {\n  contract \"MUST log in\"\n}\n\nbehavior logout \"Log out\" {\n  contract \"MUST log out\"\n}\n";

fn project(extensions: &[&str]) -> TestProject {
    TestProject::new()
        .enabling(extensions)
        .file("main.spec", BEHAVIORS)
}

/// `$ specforge args`, `[exit N]`, then both streams, with the root redacted.
fn printed(project: &TestProject, args: &[&str]) -> String {
    let run = Cli::on(project).run(args);
    let root = project.root();
    let mut spellings = vec![root.display().to_string()];
    if let Ok(canonical) = std::fs::canonicalize(root) {
        spellings.push(canonical.display().to_string());
    }
    spellings.sort_by_key(|path| std::cmp::Reverse(path.len()));
    let mut text = format!(
        "$ specforge {}\n[exit {}]\n[stdout]\n{}[stderr]\n{}\n",
        args.join(" "),
        run.code,
        run.stdout,
        run.stderr
    );
    for path in spellings {
        text = text.replace(&path, "<root>");
    }
    text
}

#[test]
fn analyze_and_trace_refusals_print_as_pinned() {
    let both = ["@specforge/software", "@specforge/testing"];
    let p = project(&both).file("bad.json", "{not json");
    let q = project(&["@specforge/software"]);
    let r = project(&both).file("specforge-report.json", "{not json");

    let mut out = String::new();
    for args in [
        &["analyze", "contrakts"][..],
        &["analyze", "contrakts", "--format", "json"],
        &["analyze", "zzzzzz", "--format", "json"],
        &["analyze", "--min", "150"],
        &["analyze", "--min", "150", "--format", "json"],
    ] {
        out.push_str(&printed(&p, args));
    }
    for args in [
        &["analyze", "--min", "50"][..],
        &["analyze", "--min", "50", "--format", "json"],
    ] {
        out.push_str(&printed(&q, args));
    }
    for args in [
        &["analyze", "--min", "50"][..],
        &["analyze", "--min", "50", "--format", "json"],
        &["analyze", "--test-results", "none.json"],
        &["analyze", "--test-results", "none.json", "--format", "json"],
        &["analyze", "--test-results", "bad.json", "--format", "json"],
    ] {
        out.push_str(&printed(&p, args));
    }
    for args in [&["analyze"][..], &["analyze", "--format", "json"]] {
        out.push_str(&printed(&r, args));
    }
    out.push_str(&printed(
        &p,
        &["analyze", "contrakts", "--min", "150", "--format", "json"],
    ));
    insta::assert_snapshot!("analyze_refusals", out);

    let mut out = String::new();
    for args in [
        &["trace", "logn"][..],
        &["trace", "logn", "--format", "human"],
        &["trace", "zzzzzzzz"],
        &["trace", "zzzzzzzz", "--format", "human"],
    ] {
        out.push_str(&printed(&p, args));
    }
    insta::assert_snapshot!("trace_refusals", out);
}
