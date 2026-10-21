//! A format call lends the served project's field forms; it loads no extension the session holds.

use crate::support::extension::{TestExtension, runtime_of};
use crate::support::project::{Serve, TestProject};
use serde_json::json;
use std::sync::Arc;

#[test]
#[ignore = "flipped by T3"]
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
