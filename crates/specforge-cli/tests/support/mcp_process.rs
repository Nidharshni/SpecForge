//! The one `specforge mcp` stdio session helper: for the tests of the binary contract (ADR 0062 D5), which are
//! process tests (`[ROOT]` resolved from the working directory, stdio, the process environment). Every other test
//! reaches MCP replies in process (`specforge_mcp::testing::Served::rooted`).

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;
use specforge_project::testing::TestProject;

/// A JSON-RPC request line.
pub fn request(id: u64, method: &str, params: Value) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

/// The `initialize` a standard MCP client sends first: no `projectRoot`, so the server compiles the
/// `specforge mcp <root>` argument.
pub fn initialize(id: u64) -> String {
    request(
        id,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "e2e", "version": "0"}
        }),
    )
}

/// `specforge mcp <root_arg>` run in `dir`, fed exactly `requests` (an `initialize` first, when the test wants
/// one), under the project's isolated environment plus `envs`; every line it wrote, parsed.
pub fn session(
    project: &TestProject,
    dir: &Path,
    root_arg: &str,
    requests: &[String],
    envs: &[(&str, String)],
) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_specforge"));
    command.args(["mcp", root_arg]).current_dir(dir);
    for key in super::cli::REMOVED {
        command.env_remove(key);
    }
    command
        .env("HOME", project.home())
        .env("USERPROFILE", project.home())
        .env("SPECFORGE_WASMTIME_CACHE", super::cli::compile_cache());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start specforge mcp");

    let stdin = child.stdin.as_mut().unwrap();
    for req in requests {
        writeln!(stdin, "{req}").unwrap();
    }
    stdin.flush().unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}
