//! `specforge.remove_extension` judges dependents as `specforge remove` does: a builtin another enabled
//! extension requires is refused with E027, and `force` removes it (ADR 0041). Builtins declare peers, so
//! this runs over the real surface (the locked requirers run in process: ops `locked_requirers.rs`).

use crate::support::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;

#[specforge_test(
    behavior = "provide_mcp_remove_extension_tool",
    verify = "remove_extension with force removes an extension another requires, which it refuses with E027 without it"
)]
fn remove_extension_refuses_a_builtin_another_requires_with_e027_unless_forced() {
    let mut served = TestProject::new()
        .enabling(&["@specforge/software", "@specforge/formal"])
        .file("main.spec", "")
        .serve_components();
    let config = served.root().join("specforge.json");
    let before = std::fs::read(&config).unwrap();

    let resp = call_tool(
        &mut served,
        "specforge.remove_extension",
        json!({"name": "@specforge/software"}),
    );
    let error = crate::tool_errors::mcp_error(&resp);
    assert_eq!(error["diagnostic"]["code"], "E027", "{error}");
    assert!(
        error["diagnostic"]["message"]
            .as_str()
            .unwrap()
            .contains("@specforge/formal"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(&config).unwrap(),
        before,
        "a refusal writes nothing"
    );

    let resp = call_tool(
        &mut served,
        "specforge.remove_extension",
        json!({"name": "@specforge/software", "force": true}),
    );
    assert_eq!(resp["result"]["isError"], false, "{resp}");
    let after: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(after["extensions"], json!(["@specforge/formal"]));
}
