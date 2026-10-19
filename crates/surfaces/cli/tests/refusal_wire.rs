//! What the CLI prints for the refusals of analyze and trace, pinned once before any of them moves (round 13,
//! plan 10). ADR 0029 D9a: the human rendering and the streams are the binary contract, so each run is printed
//! whole (`$ specforge …`, the exit code, both streams), the project root redacted.

use crate::support::{Cli, TestProject, mcp_process};
use std::path::Path;

const BEHAVIORS: &str = "behavior login \"Log in\" {\n  contract \"MUST log in\"\n}\n\nbehavior logout \"Log out\" {\n  contract \"MUST log out\"\n}\n";

fn project(extensions: &[&str]) -> TestProject {
    TestProject::new()
        .enabling(extensions)
        .file("main.spec", BEHAVIORS)
}

/// `text` with `root` (as spelled and canonical) redacted.
fn redacted(mut text: String, root: &Path) -> String {
    let mut spellings = vec![root.display().to_string()];
    if let Ok(canonical) = std::fs::canonicalize(root) {
        spellings.push(canonical.display().to_string());
    }
    spellings.sort_by_key(|path| std::cmp::Reverse(path.len()));
    for path in spellings {
        text = text.replace(&path, "<root>");
    }
    text
}

/// `$ specforge args`, `[exit N]`, then both streams.
fn transcript(args: &[&str], run: &crate::support::Run) -> String {
    format!(
        "$ specforge {}\n[exit {}]\n[stdout]\n{}[stderr]\n{}\n",
        args.join(" "),
        run.code,
        run.stdout,
        run.stderr
    )
}

/// `$ specforge args`, `[exit N]`, then both streams, with the root redacted.
fn printed(project: &TestProject, args: &[&str]) -> String {
    let run = Cli::on(project).run(args);
    redacted(transcript(args, &run), project.root())
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

const HELLO: &str = "spec \"proj\" {\n  version \"0.1.0\"\n}\n\ntype user \"User account\" {\n  status draft\n  verify \"rejects an empty email\"\n}\n\nbehavior authenticate_user \"Authenticate a user with credentials\" {\n  status   draft\n  category auth\n  contract \"Given valid credentials, returns an auth token\"\n  produces [user_logged_in]\n  verify \"rejects invalid password\"\n}\n\nevent user_logged_in \"User successfully logged in\" {\n  payload user\n  verify \"is emitted once per successful login\"\n}\n";
const AUDIT: &str = "behavior audit_login \"Audit a login\" {\n  status   draft\n  category auth\n  contract \"Records each login\"\n  consumes [user_logged_in]\n  verify \"records the login\"\n}\n";

/// A project like `specforge init` makes, plus `spec/a.spec`, which a rename of `user_logged_in` edits first.
fn changeable() -> TestProject {
    let project = TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|config| config["spec_root"] = "spec".into())
        .file("spec/hello.spec", HELLO)
        .file("spec/a.spec", AUDIT);
    Cli::on(&project).run(&["check"]);
    project
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Whether a 0444 file still opens for writing (this user is root): nothing to pin then.
#[cfg(unix)]
fn permissions_do_not_bind(dir: &Path) -> bool {
    let probe = dir.join("probe");
    std::fs::write(&probe, b"").unwrap();
    chmod(&probe, 0o444);
    let writable = std::fs::OpenOptions::new().write(true).open(&probe).is_ok();
    chmod(&probe, 0o644);
    writable
}

/// `$ specforge args` from `dir` with no `--path` (init), `dir` redacted.
#[cfg(unix)]
fn printed_in(dir: &Path, args: &[&str]) -> String {
    let bare = TestProject::new();
    let run = Cli::on(&bare).in_dir(dir).bare(args);
    redacted(transcript(args, &run), dir)
}

/// The MCP tool calls, each reply's error printed as the server answered it, the root redacted.
#[cfg(unix)]
fn printed_mcp(project: &TestProject, calls: &[(&str, serde_json::Value)]) -> String {
    let mut requests = vec![mcp_process::initialize(0)];
    for (id, (name, arguments)) in calls.iter().enumerate() {
        requests.push(mcp_process::request(
            id as u64 + 1,
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        ));
    }
    let responses = Cli::on(project).mcp_session(&requests);
    let mut text = String::new();
    for (id, (name, _)) in calls.iter().enumerate() {
        let response = mcp_process::find_response(&responses, id as u64 + 1)
            .unwrap_or_else(|| panic!("no reply to {name}: {responses:?}"));
        text.push_str(&format!(
            "$ mcp {name}\n{}\n",
            mcp_process::tool_error(response)
        ));
    }
    redacted(text, project.root())
}

#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "report_command_outcome",
    verify = "a project file a command could not write, read or found changed is file_unwritable, file_unreadable or file_changed on every command"
)]
fn project_change_refusals_print_as_pinned() {
    use std::fs;
    let scratch = tempfile::TempDir::new().unwrap();
    if permissions_do_not_bind(scratch.path()) {
        return;
    }
    let mut out = String::new();

    // A typed edit cannot write a file.
    let p = changeable();
    let hello = p.root().join("spec/hello.spec");
    chmod(&hello, 0o444);
    out.push_str(&printed(&p, &["set", "user", "status", "active"]));
    out.push_str(&printed(
        &p,
        &["set", "user", "status", "active", "--format", "json"],
    ));
    chmod(&hello, 0o644);

    // A typed edit cannot create a file.
    let p = changeable();
    let spec = p.root().join("spec");
    chmod(&spec, 0o555);
    out.push_str(&printed(
        &p,
        &["new", "type", "account", "Account", "--in", "fresh.spec"],
    ));
    chmod(&spec, 0o755);

    // The lock plan cannot write specforge.json.
    let p = changeable();
    let config = p.root().join("specforge.json");
    chmod(&config, 0o444);
    out.push_str(&printed(&p, &["add", "@specforge/product"]));
    out.push_str(&printed(
        &p,
        &["add", "@specforge/product", "--format", "json"],
    ));
    chmod(&config, 0o644);

    // init cannot write .gitignore, cannot write its config, cannot read .gitignore.
    let demo = scratch.path().join("demo");
    fs::create_dir(&demo).unwrap();
    fs::write(demo.join(".gitignore"), "target/\n").unwrap();
    chmod(&demo.join(".gitignore"), 0o444);
    out.push_str(&printed_in(&demo, &["init"]));
    out.push_str(&printed_in(&demo, &["init", "--format", "json"]));
    chmod(&demo.join(".gitignore"), 0o644);
    let demo2 = scratch.path().join("demo2");
    fs::create_dir(&demo2).unwrap();
    chmod(&demo2, 0o555);
    out.push_str(&printed_in(&demo2, &["init"]));
    chmod(&demo2, 0o755);
    let demo3 = scratch.path().join("demo3");
    fs::create_dir(&demo3).unwrap();
    fs::write(demo3.join(".gitignore"), "target/\n").unwrap();
    chmod(&demo3.join(".gitignore"), 0o000);
    out.push_str(&printed_in(&demo3, &["init"]));
    chmod(&demo3.join(".gitignore"), 0o644);

    // format writes each file alone: its own failure, the code ADR 0021 D7 documents.
    let p = changeable();
    let hello = p.root().join("spec/hello.spec");
    fs::write(&hello, format!("{HELLO}\n\n\n")).unwrap();
    chmod(&hello, 0o444);
    out.push_str(&printed(&p, &["format"]));
    chmod(&hello, 0o644);

    // Over MCP the code is the kind; the message is what differs.
    let p = changeable();
    let hello = p.root().join("spec/hello.spec");
    chmod(&hello, 0o444);
    out.push_str(&printed_mcp(
        &p,
        &[
            (
                "specforge.rename",
                serde_json::json!({"entity_id": "user_logged_in", "new_name": "login_done"}),
            ),
            (
                "specforge.edit",
                serde_json::json!({"edits": [
                    {"op": "set", "entity_id": "audit_login", "field": "status", "value": "active"},
                    {"op": "set", "entity_id": "user", "field": "status", "value": "active"},
                ]}),
            ),
        ],
    ));
    chmod(&hello, 0o644);

    insta::assert_snapshot!("project_change_refusals", out);
}
