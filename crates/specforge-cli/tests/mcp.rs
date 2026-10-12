//! `specforge mcp` as a process: the binary contract (ADR 0029 D9): its PATH
//! argument, its standard streams, its exit. What it serves is proven in
//! `specforge-mcp`, the transport in its `tests/stdio.rs`.

use crate::e2e_fixtures::*;
use specforge_test_macros::test as specforge_test;
use std::io::Write;
use std::path::Path;
use std::process::{ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn specforge_binary() -> Command {
    Command::new(assert_cmd::cargo_bin!("specforge"))
}

/// `specforge mcp <path>` fed `input` and closed: every stdout line parsed
/// as JSON, stderr, and the exit code.
fn run(path: &Path, input: &[u8]) -> (Vec<serde_json::Value>, String, Option<i32>) {
    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start specforge mcp");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let lines = String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("stdout holds only JSON-RPC lines ({e}): {line}"))
        })
        .collect();
    (
        lines,
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

/// The requests as the lines of one input.
fn input(requests: &[String]) -> Vec<u8> {
    let mut input = Vec::new();
    for request in requests {
        input.extend_from_slice(request.as_bytes());
        input.push(b'\n');
    }
    input
}

/// Every line a child wrote to `stdout`, forwarded on a channel by a thread,
/// so a reply that never comes (a missing flush) fails after a timeout
/// instead of hanging the test.
fn lines_on_a_channel(stdout: ChildStdout) -> mpsc::Receiver<String> {
    use std::io::{BufRead, BufReader};
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

/// The next line on `lines`, parsed as JSON.
fn next_json(lines: &mpsc::Receiver<String>) -> serde_json::Value {
    let line = lines
        .recv_timeout(Duration::from_secs(30))
        .expect("a line within 30 s: a reply was not flushed");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
}

#[specforge_test(
    behavior = "mcp_initialize",
    verify = "a client that names no projectRoot is served the project at or above the path specforge mcp was started with"
)]
fn mcp_serves_the_project_at_or_above_its_path() {
    let dir = setup_project_with_config(
        r#"{"name":"t","version":"0.1.0","extensions":[],"spec_root":"spec"}"#,
        &[("a.spec", r#"behavior alpha "A" { contract "first" }"#)],
    );
    for path in [dir.path().to_path_buf(), dir.path().join("spec")] {
        let list = mcp_request(
            1,
            "tools/call",
            serde_json::json!({"name": "specforge.list", "arguments": {}}),
        );
        let (lines, stderr, code) = run(&path, &input(&[mcp_initialize(0), list]));
        let listed = find_response(&lines, 1).expect("the list reply");
        let content = parse_tool_content(listed);
        let ids: Vec<&str> = content["entities"]
            .as_array()
            .unwrap_or_else(|| panic!("no entities: {listed}"))
            .iter()
            .map(|entity| entity["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["alpha"], "served from {}: {listed}", path.display());
        assert_eq!(stderr, "");
        assert_eq!(code, Some(0));
    }
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "specforge mcp serves its standard input and output and exits 0 when its input ends"
)]
fn mcp_answers_each_request_on_stdout_and_exits_0_at_the_end_of_its_input() {
    let dir = setup_project(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);
    let initialized = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let requests = [
        mcp_initialize(0),
        initialized.to_string(),
        mcp_request(1, "tools/list", serde_json::json!({})),
        mcp_request(2, "ping", serde_json::json!({})),
    ];

    let (lines, stderr, code) = run(dir.path(), &input(&requests));

    let ids: Vec<&serde_json::Value> = lines.iter().map(|line| &line["id"]).collect();
    assert_eq!(ids, [0, 1, 2], "nothing unsolicited: {lines:?}");
    assert!(
        lines[0]["result"]["capabilities"]["tools"].is_object(),
        "{}",
        lines[0]
    );
    assert!(
        !lines[1]["result"]["tools"].as_array().unwrap().is_empty(),
        "{}",
        lines[1]
    );
    assert_eq!(lines[2]["result"], serde_json::json!({}));
    assert_eq!(stderr, "");
    assert_eq!(code, Some(0));
}

/// A stdio client subscribed to the graph gets `specforge/graphChanged` on
/// stdout once the project changed: each reply arrives before the client's
/// next request, a response before the notifications it queued.
#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "a client reading specforge mcp's output line by line gets each response before it sends its next request, the notifications it queued after it"
)]
fn mcp_writes_each_response_before_the_next_request_and_its_notifications_after_it() {
    let dir = setup_project(&[("spec/base.spec", r#"behavior base "B" { contract "b" }"#)]);
    let mut child = specforge_binary()
        .args(["mcp"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");
    let mut stdin = child.stdin.take().unwrap();
    let lines = lines_on_a_channel(child.stdout.take().unwrap());
    let send = |stdin: &mut std::process::ChildStdin, request: String| {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
    };

    send(&mut stdin, mcp_initialize(0));
    assert_eq!(next_json(&lines)["id"], 0);
    send(
        &mut stdin,
        mcp_request(
            1,
            "resources/subscribe",
            serde_json::json!({"uri": "specforge://graph"}),
        ),
    );
    assert_eq!(next_json(&lines)["id"], 1);

    // A new entity on disk: the next read brings the project up to date,
    // with no watch running.
    std::fs::write(
        dir.path().join("spec/added.spec"),
        r#"behavior added "A" { contract "a" }"#,
    )
    .unwrap();
    send(
        &mut stdin,
        mcp_request(
            2,
            "resources/read",
            serde_json::json!({"uri": "specforge://diagnostics"}),
        ),
    );
    let response = next_json(&lines);
    assert_eq!(response["id"], 2, "the response comes first: {response}");
    let updated = next_json(&lines);
    assert_eq!(
        updated["method"], "notifications/resources/updated",
        "{updated}"
    );
    assert_eq!(updated["params"]["uri"], "specforge://graph", "{updated}");
    let changed = next_json(&lines);
    assert_eq!(changed["method"], "specforge/graphChanged", "{changed}");
    assert!(changed.get("id").is_none(), "{changed}");

    drop(stdin);
    assert!(child.wait().unwrap().success());
}

/// A line that is not UTF-8 is not JSON: answered with -32700, and the
/// session goes on.
#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "specforge mcp answers a line that is not UTF-8 instead of ending the session"
)]
fn mcp_answers_a_line_that_is_not_utf8_and_keeps_serving() {
    let dir = setup_project(&[("main.spec", r#"behavior alpha "A" { contract "first" }"#)]);
    let ping = |id: u64| mcp_request(id, "ping", serde_json::json!({}));
    let mut bytes = format!("{}\n", ping(1)).into_bytes();
    bytes.extend_from_slice(b"\xff\xfe\n");
    bytes.extend_from_slice(format!("{}\n", ping(2)).as_bytes());

    let (lines, stderr, code) = run(dir.path(), &bytes);

    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0]["id"], 1);
    assert!(lines[1]["id"].is_null(), "{}", lines[1]);
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert_eq!(lines[1]["error"]["message"], "Parse error");
    assert_eq!(lines[2]["id"], 2);
    assert_eq!(stderr, "");
    assert_eq!(code, Some(0));
}
