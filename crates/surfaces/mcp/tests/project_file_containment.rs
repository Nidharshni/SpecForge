//! Every project file an operation opens goes through the call's containment
//! (ADR 0104, ADR 0112).
//!
//! These are the escapes that no earlier fix reached: a file the operation
//! opens by name follows whatever symlink is there. Each pin is ignored until
//! the ticket that closes the escape flips it.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A second directory: anything not under the served project is outside.
fn outside() -> TempDir {
    TempDir::new().unwrap()
}

/// `root` served, initialized as a handshake client does.
fn serve(root: &Path) -> McpServer {
    let mut server = McpServer::with_project_root(root.to_path_buf());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

/// A project with one source file and no extension.
fn plain_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("spec/main.spec"),
        "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n",
    )
    .unwrap();
    dir
}

/// The refusal a containment violation is: `invalid_input`, optionally on `argument`.
fn assert_refused(reply: &Value, argument: Option<&str>) {
    let error = crate::tool_errors::mcp_error(reply);
    assert_eq!(error["code"], "invalid_input", "{error}");
    if let Some(argument) = argument {
        assert_eq!(error["argument"], argument, "{error}");
    }
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).unwrap();
}

/// R1: `specforge-report.json` is a link to a file outside.
#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect reads and writes no file through a symlink that leaves the project"
)]
fn collect_writes_no_report_through_a_symlink_that_leaves_the_project() {
    let project = crate::operations_mgmt::collect_project();
    let outside = outside();
    let victim = outside.path().join("victim.json");
    fs::write(&victim, "ORIGINAL").unwrap();
    link(&victim, &project.path().join("specforge-report.json"));
    let mut server = serve(project.path());

    let reply = call_tool(&mut server, "specforge.collect", json!({}));

    assert_refused(&reply, None);
    assert_eq!(fs::read_to_string(&victim).unwrap(), "ORIGINAL");
}

/// R4: a runner's captured output is a link to a file outside.
#[cfg(unix)]
#[specforge_test_macros::test(
    behavior = "provide_mcp_collect_tool",
    verify = "specforge.collect reads and writes no file through a symlink that leaves the project"
)]
fn collect_reads_no_captured_output_through_a_symlink_that_leaves_the_project() {
    let project = crate::operations_mgmt::collect_project();
    let outside = outside();
    let secret = outside.path().join("secret.txt");
    fs::write(&secret, "test works ... ok\n").unwrap();
    fs::remove_file(project.path().join("target/specforge/t.json")).unwrap();
    link(
        &secret,
        &project
            .path()
            .join("target/specforge/cargo-test.stdout.txt"),
    );
    let mut server = serve(project.path());

    let reply = call_tool(&mut server, "specforge.collect", json!({}));

    assert_refused(&reply, None);
    assert!(!project.path().join("specforge-report.json").exists());
}

/// R2: the manifest's temporary file is a link to a file outside.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation reads and writes no file through a symlink that leaves the project"
)]
fn infer_session_writes_no_manifest_through_a_temporary_file_that_leaves_the_project() {
    let project = plain_project();
    let outside = outside();
    let victim = outside.path().join("victim2.json");
    fs::write(&victim, "ORIGINAL2").unwrap();
    link(&victim, &project.path().join("specforge-infer.json.tmp"));
    let mut server = serve(project.path());

    let reply = call_tool(
        &mut server,
        "specforge.infer_session",
        json!({"action": "start"}),
    );

    assert_ne!(reply["result"]["isError"], true, "{reply}");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "ORIGINAL2");
    let manifest = project.path().join("specforge-infer.json");
    assert!(fs::symlink_metadata(&manifest).unwrap().is_file());
    assert!(
        fs::read_to_string(&manifest)
            .unwrap()
            .contains("\"sessions\"")
    );
    assert!(
        fs::symlink_metadata(project.path().join("specforge-infer.json.tmp")).is_err(),
        "no temporary file is left"
    );
}

/// R5: the manifest itself is a link to a file outside.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a file the project keeps at its root is read only from inside the project"
)]
fn infer_progress_reads_no_manifest_through_a_symlink_that_leaves_the_project() {
    let project = plain_project();
    let outside = outside();
    let manifest = outside.path().join("m.json");
    fs::write(
        &manifest,
        r#"{"version":1,"sessions":[{"session_id":"OUTSIDE-SESSION","started_at":"2026-01-01T00:00:00Z","agent":"a","status":"paused"}],"source_roots":[]}"#,
    )
    .unwrap();
    link(&manifest, &project.path().join("specforge-infer.json"));
    let mut server = serve(project.path());

    let reply = call_tool(&mut server, "specforge.infer_progress", json!({}));

    assert_refused(&reply, None);
    assert!(!reply.to_string().contains("OUTSIDE-SESSION"), "{reply}");
    assert!(reply.to_string().contains("E071"), "{reply}");
}

/// R3: a `test_results` that resolves outside the project, in every spelling.
#[specforge_test_macros::test(
    behavior = "provide_mcp_analyze_tool",
    verify = "a test_results that resolves outside the call's project is refused before it is read"
)]
fn analyze_test_results_outside_the_project_is_refused_before_it_is_read() {
    let scene = TempDir::new().unwrap();
    let project = scene.path().join("proj");
    let elsewhere = scene.path().join("outside");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(
        project.join("specforge.json"),
        r#"{"name":"p","version":"0.1.0"}"#,
    )
    .unwrap();
    fs::write(elsewhere.join("r.json"), r#"{"results":{}}"#).unwrap();
    fs::write(elsewhere.join("bad.json"), "not json").unwrap();

    #[allow(unused_mut, reason = "only unix adds the symlink spelling")]
    let mut spellings: Vec<(&str, String)> = vec![
        ("relative", "../outside/r.json".to_string()),
        ("absolute", elsewhere.join("r.json").display().to_string()),
        (
            "absolute bad",
            elsewhere.join("bad.json").display().to_string(),
        ),
        (
            "absolute missing",
            elsewhere.join("missing.json").display().to_string(),
        ),
    ];
    #[cfg(unix)]
    {
        link(&elsewhere, &project.join("link"));
        spellings.push(("symlink", "link/r.json".to_string()));
    }

    let mut failures = Vec::new();
    for (how, name) in &spellings {
        let mut server = serve(&project);
        let reply = call_tool(
            &mut server,
            "specforge.analyze",
            json!({"pass": "coverage", "test_results": name}),
        );
        let refused = reply["result"]["isError"] == true
            && serde_json::from_str::<Value>(
                reply["result"]["content"][0]["text"].as_str().unwrap_or(""),
            )
            .is_ok_and(|e| e["code"] == "invalid_input" && e["argument"] == "test_results");
        if !refused {
            failures.push(format!(
                "{how}: not refused as invalid_input on test_results: {reply}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// R6: the backup and the temporary file of a migrated source are links to files outside.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation reads and writes no file through a symlink that leaves the project"
)]
fn migrate_writes_no_backup_or_temporary_file_through_a_symlink_that_leaves_the_project() {
    let project = plain_project();
    let outside = outside();
    let source = project.path().join("spec/ghost.spec");
    let old = "// specforge-format: 0.9\nbehavior ghost \"G\" {\n}\n";
    fs::write(&source, old).unwrap();
    fs::write(outside.path().join("bak"), "BAK").unwrap();
    fs::write(outside.path().join("tmp"), "TMP").unwrap();
    link(
        &outside.path().join("bak"),
        &project.path().join("spec/ghost.spec.bak"),
    );
    link(
        &outside.path().join("tmp"),
        &project.path().join("spec/ghost.spec.tmp"),
    );
    let mut server = serve(project.path());

    let reply = call_tool(&mut server, "specforge.migrate", json!({}));

    assert_refused(&reply, None);
    assert_eq!(
        fs::read_to_string(outside.path().join("bak")).unwrap(),
        "BAK"
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("tmp")).unwrap(),
        "TMP"
    );
    assert!(fs::symlink_metadata(&source).unwrap().is_file());
    assert_eq!(fs::read_to_string(&source).unwrap(), old);
}

/// Every `.rs` file under `dir`, with its text up to the test module.
fn sources(dir: &Path, out: &mut Vec<(String, String)>, base: &Path) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out, base);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let name = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let text = fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            let end = (0..lines.len())
                .find(|&i| {
                    lines[i].starts_with("#[cfg(test)]")
                        && lines.get(i + 1).is_some_and(|next| next.contains("mod "))
                })
                .unwrap_or(lines.len());
            out.push((name, lines[..end].join("\n")));
        }
    }
}

/// A tool builds no bound and no asker of its own and opens no file: the call's project hands its
/// caller (ADR 0112 D4, ADR 0124). The exceptions say why.
#[test]
fn every_caller_a_tool_uses_comes_from_the_call() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files, &src);
    let within = [
        "containment.rs",
        "target/call.rs",
        "tools/outline.rs", // the spec root bounds a file of the outline
    ];
    let mut problems = Vec::new();
    for (name, text) in &files {
        if name.ends_with("tests.rs") || name.contains("/tests/") {
            continue;
        }
        for (number, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains("Containment::within(") && !within.contains(&name.as_str()) {
                problems.push(format!("src/{name}:{}: builds its own bound", number + 1));
            }
            if line.contains("Asking::") && name != "target/call.rs" {
                problems.push(format!("src/{name}:{}: names its own asker", number + 1));
            }
            if name.starts_with("tools/") && line.contains("project.containment(") {
                problems.push(format!(
                    "src/{name}:{}: reads the project's containment, not its caller",
                    number + 1
                ));
            }
            if line.contains("std::fs::") {
                problems.push(format!("src/{name}:{}: opens a file itself", number + 1));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
