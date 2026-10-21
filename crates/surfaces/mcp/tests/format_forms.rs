//! A format call lends the served project's field forms; it loads no extension the session holds.

use crate::support::extension::{TestExtension, runtime_of};
use crate::support::project::{Serve, TestProject};
use serde_json::json;
use specforge_mcp::testing::Served;
use std::sync::Arc;

const TAGGED: &str =
    "behavior bb \"B\" {\n  contract \"The system MUST x\"\n  category \"auth\"\n}\n";

#[specforge_test_macros::test(
    behavior = "provide_mcp_format_tool",
    verify = "a format call loads no extension the served project holds"
)]
fn a_format_call_loads_no_extension_the_served_project_holds() {
    let runtime = Arc::new(runtime_of(&[TestExtension::new()]));
    let mut server = TestProject::new()
        .file("a.spec", "behavior b \"B\" {\n  contract \"x\"\n}\n")
        .serve_in(runtime.clone() as specforge_project::SharedRuntime);
    runtime.clear_calls();

    server.tool("specforge.format", json!({"check": true}));

    let handshakes = runtime
        .calls()
        .iter()
        .filter(|call| call.export == "__handshake")
        .count();
    assert_eq!(handshakes, 0, "{:?}", runtime.calls());
}

#[specforge_test_macros::test(
    behavior = "provide_mcp_format_tool",
    verify = "a format call writes the served project's values in its forms"
)]
fn a_format_call_writes_the_served_projects_values_in_its_forms() {
    let mut server = Served::rooted(
        TestProject::new()
            .enabling(&["@specforge/software"])
            .file("spec/a.spec", TAGGED),
    );

    let reply = server.tool("specforge.format", json!({"check": true}));

    assert_eq!(reply["changed_files"], json!(["spec/a.spec"]), "{reply}");
}
