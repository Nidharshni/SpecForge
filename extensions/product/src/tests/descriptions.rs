//! A declared description is read by the CLI and by MCP, so it names the request's words, never a CLI flag
//! (ADR 0135).

use specforge_extension_sdk::testing::MockHost;

/// Every string of `value` that holds `--`, as `path: text`.
fn flagged(value: &serde_json::Value, path: &str, found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) if text.contains("--") => {
            found.push(format!("{path}: {text}"));
        }
        serde_json::Value::Array(items) => {
            for (at, item) in items.iter().enumerate() {
                flagged(item, &format!("{path}/{at}"), found);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                flagged(item, &format!("{path}/{key}"), found);
            }
        }
        _ => {}
    }
}

#[test]
#[ignore = "flipped by T3"]
fn no_product_description_names_a_cli_flag() {
    let payload: serde_json::Value = serde_json::from_str(
        &MockHost::new(crate::specforge_extension_build())
            .describe_json("surfaces")
            .unwrap(),
    )
    .unwrap();
    let mut found = Vec::new();
    flagged(&payload, "", &mut found);
    assert!(
        found.is_empty(),
        "{} declared strings name a CLI flag (ADR 0135 D5):\n{}",
        found.len(),
        found.join("\n")
    );
}
