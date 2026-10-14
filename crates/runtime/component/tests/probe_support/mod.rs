//! The sandbox probe (`fixtures/sandbox-probe`, `specforge_component::committed::PROBE`)
//! and what a test needs to run it: the bait it is told about and must not
//! reach, and what its report says. Shared by `sandbox_probe.rs` and
//! `sandbox_limits.rs`.

// Each test binary uses the part of the probe it needs.
#![allow(dead_code)]

use serde_json::{Value, json};
use specforge_component::ComponentRuntime;
use specforge_protocol_types::{CommandInput, RawGraph};
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};
use std::net::TcpListener;

pub const PROBE: &str = "@test/probe";

pub fn probe_runtime() -> ComponentRuntime {
    let path = specforge_component::committed::PROBE.path();
    let blob = std::fs::read(path).expect("vendored sandbox probe blob");
    let runtime = ComponentRuntime::new();
    runtime.load(PROBE, &blob).expect("the probe instantiates");
    runtime
}

/// The surfaces the probe declares, as it describes them.
pub fn declared_surfaces(runtime: &ComponentRuntime) -> Value {
    match runtime.call_export(PROBE, "__describe", br#"{"category":"surfaces"}"#) {
        WasmCallResult::Ok(bytes) => {
            let envelope: Value = serde_json::from_slice(&bytes).unwrap();
            envelope["items"][0].clone()
        }
        WasmCallResult::Trap(trap) => panic!("describe trapped: {trap:?}"),
    }
}

/// A directory holding `secret.txt` and a port with a listener behind it:
/// what the probe is told about and must not reach.
pub struct Bait {
    dir: tempfile::TempDir,
    listener: TcpListener,
}

impl Bait {
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        Bait { dir, listener }
    }

    pub fn dir(&self) -> String {
        self.dir.path().display().to_string()
    }

    pub fn port(&self) -> u16 {
        self.listener.local_addr().unwrap().port()
    }

    /// Nothing reached the bait: no file was written and no connection
    /// arrived.
    pub fn untouched(&self) {
        assert!(
            !self.dir.path().join("probe.txt").exists(),
            "the probe wrote a file"
        );
        assert_eq!(
            self.listener.accept().map(|_| ()).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "the probe connected"
        );
    }
}

/// `report` says no capability was granted: every attempt failed, and the
/// environment, arguments and stdin were empty.
pub fn granted_nothing(report: &Value, attempts: &[&str]) {
    for attempt in attempts {
        assert_eq!(report[attempt]["granted"], false, "{attempt}: {report}");
    }
    assert_eq!(report["env_vars"], 0, "{report}");
    assert_eq!(report["args"], 0, "{report}");
    assert_eq!(report["stdin_bytes"], 0, "{report}");
}

/// The probe command's input: the bait's port as its arg, the bait's
/// directory as the project root, an empty graph.
pub fn command_input(bait: &Bait) -> CommandInput<RawGraph> {
    CommandInput {
        args: json!({"port": bait.port()}).as_object().unwrap().clone(),
        cwd: bait.dir(),
        graph: RawGraph::new(r#"{"nodes":[],"edges":[]}"#.to_string()).unwrap(),
        ..CommandInput::default()
    }
}

pub const ALL: &[&str] = &[
    "read_root",
    "read_dir",
    "read_file",
    "write_file",
    "connect",
    "resolve",
];
