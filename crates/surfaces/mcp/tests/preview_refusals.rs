//! A preview refused before its handler runs, for no project or a `path` naming none, is no mutation (ADR 0022
//! M2, ADR 0229): no `files_written`, nothing recorded. The same call asking for a run is a failed mutation.

use std::ops::DerefMut;

use serde_json::{Value, json};
use specforge_mcp::McpServer;
use tempfile::TempDir;

use crate::support::replies::reading;
use crate::support::{Logs, call_tool, events, unserved};
use crate::tool_errors::mcp_error;

/// Each mutation that can preview: arguments asking for a preview, then the same call asking for a run.
fn previews() -> Vec<(&'static str, Value, Value)> {
    let retitle = json!([{"op": "retitle", "entity_id": "a", "title": "A"}]);
    let rename = json!({"entity_id": "a", "new_name": "b"});
    let product = json!({"specifiers": ["@specforge/product"]});
    let remove = json!({"name": "@specforge/product"});
    let with_dry_run = |mut arguments: Value| {
        arguments["dry_run"] = json!(true);
        arguments
    };
    vec![
        ("specforge.format", json!({"check": true}), json!({})),
        (
            "specforge.format",
            json!({"diff": true}),
            json!({"diff": true, "write": true}),
        ),
        ("specforge.rename", with_dry_run(rename.clone()), rename),
        (
            "specforge.edit",
            json!({"edits": retitle.clone(), "dry_run": true}),
            json!({"edits": retitle}),
        ),
        (
            "specforge.add_extension",
            with_dry_run(product.clone()),
            product,
        ),
        (
            "specforge.remove_extension",
            with_dry_run(remove.clone()),
            remove,
        ),
        ("specforge.migrate", json!({"dry_run": true}), json!({})),
    ]
}

/// The refusal `arguments` get, and how many `mcp_mutation_completed` it recorded.
fn refused(
    server: &mut (impl Logs + DerefMut<Target = McpServer>),
    tool: &str,
    arguments: Value,
) -> (Value, usize) {
    let before = events(server, "mcp_mutation_completed").len();
    let error = mcp_error(&call_tool(server, tool, arguments));
    (
        error,
        events(server, "mcp_mutation_completed").len() - before,
    )
}

#[specforge_test_macros::test(
    behavior = "mcp_mutation_completed",
    verify = "a preview refused before its handler runs, for no project or a path naming none, lists no files_written and records nothing"
)]
fn a_preview_refused_before_it_runs_is_no_mutation() {
    let mut nothing = unserved();
    let mut served = reading();
    let elsewhere = TempDir::new().unwrap();
    for (tool, preview, run) in previews() {
        let (error, recorded) = refused(&mut nothing, tool, preview.clone());
        assert_eq!(
            error["code"], "precondition_failed",
            "{tool} {preview}: {error}"
        );
        assert!(
            error["data"].get("files_written").is_none(),
            "{tool} {preview}: {error}"
        );
        assert_eq!(recorded, 0, "{tool} {preview}");

        let mut aimed = preview.clone();
        aimed["path"] = json!(elsewhere.path().to_str().unwrap());
        let (error, recorded) = refused(&mut served, tool, aimed.clone());
        assert_eq!(error["argument"], "path", "{tool} {aimed}: {error}");
        assert!(
            error["data"].get("files_written").is_none(),
            "{tool} {aimed}: {error}"
        );
        assert_eq!(recorded, 0, "{tool} {aimed}");

        // The same call asking for a run is a failed mutation that wrote nothing.
        let (error, recorded) = refused(&mut nothing, tool, run.clone());
        assert_eq!(
            error["data"]["files_written"],
            json!([]),
            "{tool} {run}: {error}"
        );
        assert_eq!(recorded, 1, "{tool} {run}");
    }
}
