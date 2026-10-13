//! The sandbox probe: an extension whose surface exports try every
//! capability a guest could reach (directories, the environment, arguments,
//! stdin, the network) and report what they got. The host's tests run it
//! through the component runtime to prove a surface export is granted no
//! capability, even one whose declaration asks for some. Its surfaces are
//! declared with their handlers, the SDK routing the exports; the describe
//! answer for `surfaces` is served raw, as a guest built before ADR 0037
//! would give it, so each surface still carries the `sandbox` override the
//! protocol no longer has (the host reports it as W153 and grants nothing).
//! The component blob is vendored at `fixtures/sandbox-probe/wasm/sandbox_probe.wasm`
//! (refresh: `cargo run -p xtask --bin revendor`).

use serde_json::{json, Value};
use specforge_extension_sdk::prelude::*;
use std::io::Read;
use std::net::ToSocketAddrs;

#[specforge_extension_sdk::extension(
    name = "@test/probe",
    version = "0.1.0",
    short = "probe",
    description = "Sandbox probe"
)]
struct Probe;

/// The override every surface asks for: every capability. It must grant
/// none of them.
fn everything() -> Value {
    json!({"fs_read": true, "fs_write": true, "network": true})
}

impl Contributions for Probe {
    fn contribute(c: &mut ContributionsBuilder) {
        // Surfaces are described for an extension that contributes entities.
        c.kind("probe_target", |k| {
            k.description("Something to probe").testable(false);
        });
        c.command("probe", |cmd| {
            cmd.title("Probe the sandbox")
                .description("Try every capability and report what was granted")
                .arg("port", |a| {
                    a.integer().description("A port the host listens on");
                })
                .handler(probe);
        });
        c.command("trap", |cmd| {
            cmd.title("Trap")
                .description("Panic")
                .handler(|_| panic!("the probe trapped"));
        });
        c.mcp_tool("probe.tool", |t| {
            t.description("Try every capability and report what was granted")
                .input_schema(json!({"type": "object", "properties": {
                    "dir": {"type": "string"}, "port": {"type": "integer"}
                }}))
                .handler(|input| {
                    let dir = input.get("dir").and_then(Value::as_str).unwrap_or("/");
                    Ok(report(dir, input.get("port").and_then(Value::as_i64)))
                });
        });
        c.mcp_resource("probe-resource", |r| {
            r.uri_template("specforge://ext/probe/{dir}")
                .description("Try every capability and report what was granted")
                .mime_type("application/json")
                .handler(|uri| {
                    let dir = uri.strip_prefix("specforge://ext/probe").unwrap_or("/");
                    Ok(json!({"uri": uri, "sandbox": report(dir, None)}).to_string())
                });
        });
        // The surfaces as an older guest describes them, each with the
        // override the builders can no longer declare. The builders above
        // still route the handlers.
        c.raw_category(
            specforge_extension_sdk::DeclaredCategory::Surfaces,
            json!([{
                "commands": [
                    {
                        "id": "probe", "title": "Probe the sandbox",
                        "description": "Try every capability and report what was granted",
                        "export": "cmd__probe",
                        "args": [{
                            "name": "port", "arg_type": "integer", "required": false,
                            "description": "A port the host listens on"
                        }],
                        "sandbox": everything()
                    },
                    {
                        "id": "trap", "title": "Trap", "description": "Panic",
                        "export": "cmd__trap", "args": []
                    }
                ],
                "mcp_tools": [{
                    "name": "probe.tool",
                    "description": "Try every capability and report what was granted",
                    "export": "mcp__probe_tool",
                    "input_schema": {"type": "object", "properties": {
                        "dir": {"type": "string"}, "port": {"type": "integer"}
                    }},
                    "sandbox": everything()
                }],
                "mcp_resources": [{
                    "uri_template": "specforge://ext/probe/{dir}",
                    "name": "probe-resource",
                    "description": "Try every capability and report what was granted",
                    "export": "mcp__probe_resource",
                    "mime_type": "application/json",
                    "sandbox": everything()
                }]
            }]),
        );
    }
}

/// The `probe` command: its input, and what it got when it tried every
/// capability, exiting 3 (a failed MCP call).
fn probe(call: &CommandCall<'_>) -> CommandOutput {
    let port = call.integer("port");
    let args = match port {
        Some(port) => json!({"port": port}),
        None => json!({}),
    };
    let out = json!({
        "args": args,
        "cwd": call.cwd(),
        "nodes": call.graph().nodes().iter().map(|n| n.id.clone()).collect::<Vec<_>>(),
        "sandbox": report(call.cwd(), port),
    });
    CommandOutput {
        exit_code: 3,
        stdout: format!("{out}\n"),
        stderr: "probed\n".to_string(),
    }
}

/// What the guest got when it tried each capability, `dir` being a
/// directory it was told about (holding `secret.txt`) and `port` one the
/// host listens on.
fn report(dir: &str, port: Option<i64>) -> Value {
    let attempt = |result: std::io::Result<()>| match result {
        Ok(()) => json!({"granted": true}),
        Err(e) => json!({"granted": false, "error": e.to_string()}),
    };
    let mut stdin = Vec::new();
    let stdin_read = std::io::stdin().read_to_end(&mut stdin);
    json!({
        "read_root": attempt(std::fs::read_dir("/").map(|_| ())),
        "read_dir": attempt(std::fs::read_dir(dir).map(|_| ())),
        "read_file": attempt(std::fs::read(format!("{dir}/secret.txt")).map(|_| ())),
        "write_file": attempt(std::fs::write(format!("{dir}/probe.txt"), b"written")),
        "env_vars": std::env::vars_os().count(),
        "args": std::env::args_os().count(),
        "stdin_bytes": stdin_read.map(|_| stdin.len()).unwrap_or(0),
        "connect": attempt(match port {
            Some(port) => std::net::TcpStream::connect(("127.0.0.1", port as u16)).map(|_| ()),
            None => Err(std::io::Error::other("no port given")),
        }),
        "resolve": attempt(("localhost", 80).to_socket_addrs().map(|_| ())),
    })
}

specforge_extension_sdk::component_guest!(build = specforge_extension_build);
