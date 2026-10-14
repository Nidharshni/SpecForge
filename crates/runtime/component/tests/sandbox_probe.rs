//! A surface export is granted no capability. The sandbox probe
//! (`fixtures/sandbox-probe`, vendored as `probe.wasm`) declares a CLI
//! command, an MCP tool and an MCP resource whose sandbox overrides ask for
//! every capability; each export tries to list and read a directory it is
//! told about, write a file into it, read the environment, its arguments and
//! stdin, connect to a port the test listens on, and resolve a name, and
//! reports what it got. The probe runs through the component runtime the
//! host runs every extension in, called as the host dispatches each surface.

mod probe_support;

use probe_support::{
    ALL, Bait, PROBE, command_input, declared_surfaces, granted_nothing, probe_runtime,
};
use serde_json::{Value, json};
use specforge_test_macros::test as specforge_test;
use specforge_wasm::ExtensionCalls;
use specforge_wasm::protocol::load_declaration;

#[specforge_test(
    behavior = "dispatch_surface_command",
    verify = "a cmd__ export is granted no capability, whatever sandbox its declaration asks for"
)]
fn a_command_export_is_granted_no_capability() {
    let runtime = probe_runtime();
    let declared = declared_surfaces(&runtime);
    assert_eq!(
        declared["commands"][0]["sandbox"],
        json!({"fs_read": true, "fs_write": true, "network": true})
    );

    let bait = Bait::new();
    let output = ExtensionCalls::new(&runtime)
        .run_command(PROBE, "cmd__probe", &command_input(&bait))
        .expect("the probe answers");
    let out: Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(out["cwd"], bait.dir(), "told the project root");
    granted_nothing(&out["sandbox"], ALL);
    bait.untouched();
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_tool",
    verify = "an mcp__ tool export is granted no capability, whatever sandbox its declaration asks for"
)]
fn an_mcp_tool_export_is_granted_no_capability() {
    let runtime = probe_runtime();
    let declared = declared_surfaces(&runtime);
    assert_eq!(declared["mcp_tools"][0]["sandbox"]["network"], true);

    let bait = Bait::new();
    let input = json!({"dir": bait.dir(), "port": bait.port()});
    let report = ExtensionCalls::new(&runtime)
        .call_mcp_tool(PROBE, "mcp__probe_tool", &input)
        .expect("the probe answers");
    granted_nothing(&report, ALL);
    bait.untouched();
}

#[specforge_test(
    behavior = "dispatch_surface_mcp_resource",
    verify = "fs_write denied for resource contributions"
)]
fn an_mcp_resource_export_cannot_write() {
    let runtime = probe_runtime();
    let declared = declared_surfaces(&runtime);
    assert_eq!(declared["mcp_resources"][0]["sandbox"]["fs_write"], true);

    let bait = Bait::new();
    let uri = format!("specforge://ext/probe{}", bait.dir());
    let read = ExtensionCalls::new(&runtime)
        .read_mcp_resource(PROBE, "mcp__probe_resource", &uri)
        .expect("the probe answers");
    assert_eq!(read.mime_type, "application/json");
    let content: Value = serde_json::from_str(&read.content).unwrap();
    assert_eq!(content["uri"], uri);
    assert_eq!(
        content["sandbox"]["write_file"]["granted"], false,
        "{content}"
    );
    granted_nothing(
        &content["sandbox"],
        &["read_root", "read_dir", "read_file", "resolve"],
    );
    bait.untouched();
}

#[specforge_test(
    invariant = "surface_sandbox_ceiling",
    verify = "a surface export whose sandbox override asks for every capability is granted none"
)]
fn no_override_expands_the_ceiling() {
    let runtime = probe_runtime();
    let declared = declared_surfaces(&runtime);
    let everything = json!({"fs_read": true, "fs_write": true, "network": true});
    for surface in ["commands", "mcp_tools", "mcp_resources"] {
        assert_eq!(declared[surface][0]["sandbox"], everything, "{surface}");
    }

    let bait = Bait::new();
    let calls = ExtensionCalls::new(&runtime);
    let output = calls
        .run_command(PROBE, "cmd__probe", &command_input(&bait))
        .unwrap();
    let command: Value = serde_json::from_str(&output.stdout).unwrap();
    let tool = calls
        .call_mcp_tool(
            PROBE,
            "mcp__probe_tool",
            &json!({"dir": bait.dir(), "port": bait.port()}),
        )
        .unwrap();
    let uri = format!("specforge://ext/probe{}", bait.dir());
    let resource = calls
        .read_mcp_resource(PROBE, "mcp__probe_resource", &uri)
        .unwrap();
    let resource: Value = serde_json::from_str(&resource.content).unwrap();

    granted_nothing(&command["sandbox"], ALL);
    granted_nothing(&tool, ALL);
    granted_nothing(
        &resource["sandbox"],
        &[
            "read_root",
            "read_dir",
            "read_file",
            "write_file",
            "resolve",
        ],
    );
    bait.untouched();
}

#[specforge_test(
    invariant = "surface_sandbox_ceiling",
    verify = "a surface's sandbox override grants nothing and is W153 at load"
)]
fn the_probe_loads_with_one_w153_per_override() {
    let loaded = load_declaration(&probe_runtime(), PROBE).expect("the probe loads");
    assert!(
        loaded.warnings.iter().all(|warning| warning.code == "W153"),
        "no W138: {:?}",
        loaded.warnings
    );
    let items: Vec<&str> = [
        "#0.commands[probe]",
        "#0.mcp_tools[probe.tool]",
        "#0.mcp_resources[probe-resource]",
    ]
    .into_iter()
    .filter(|item| loaded.warnings.iter().any(|w| w.message.contains(item)))
    .collect();
    assert_eq!(items.len(), 3, "{:?}", loaded.warnings);
    assert_eq!(loaded.warnings.len(), 3, "{:?}", loaded.warnings);
}

/// The component runtime keeps the contract every adapter of the
/// `WasmRuntime` port keeps (`specforge_wasm::testing::assert_runtime_contract`,
/// also run over the in-process runtime): the probe's `trap` command panics.
#[specforge_test(
    behavior = "call_extension_exports",
    verify = "both runtimes report an unknown extension, an unrouted export, a guest error and a guest panic as traps"
)]
#[specforge_test(port = "WasmRuntime", verify = "WasmRuntime contract is satisfied")]
fn the_component_runtime_keeps_the_runtime_contract() {
    specforge_wasm::testing::assert_runtime_contract(&probe_runtime(), PROBE, "cmd__trap");
}

/// Whatever the export is and whatever it is told about, the host's
/// context holds: the probe tries the root, a directory and a port the test
/// offers it, and reaches none of them.
#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "an export reaches no directory: the root and a directory it is told about can be neither listed, read nor written"
)]
#[specforge_test(
    behavior = "enforce_wasm_sandbox",
    verify = "an export reaches no network: it can neither connect to a listening port nor resolve a name"
)]
#[specforge_test(
    constraint = "wasm_sandbox_enforcement",
    verify = "direct filesystem access from Wasm is blocked"
)]
#[specforge_test(
    constraint = "wasm_sandbox_enforcement",
    verify = "direct network access from Wasm is blocked"
)]
#[specforge_test(
    constraint = "wasm_sandbox_enforcement",
    verify = "an extension that tries every capability a component can reach is granted none"
)]
fn an_export_reaches_no_file_and_no_socket() {
    let runtime = probe_runtime();
    let bait = Bait::new();
    let report = ExtensionCalls::new(&runtime)
        .call_mcp_tool(
            PROBE,
            "mcp__probe_tool",
            &json!({"dir": bait.dir(), "port": bait.port()}),
        )
        .expect("the probe answers");
    granted_nothing(&report, ALL);
    bait.untouched();
}
