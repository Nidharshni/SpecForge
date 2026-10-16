//! Who can be asked, as the binary shows it (ADR 0110). A process test, because the user's key pins are read
//! from `HOME`, which only a process isolates (ADR 0029 D9a).

use crate::registry::known_keys;
use crate::support::mcp_process::{initialize, parse_tool_content, request, session};
use crate::support::registry::{Package, greet_wasm, project_on, serve};
use serde_json::json;

#[test]
fn mcp_add_extension_refusing_a_key_change_names_the_commands_that_can_consent() {
    let pinned = specforge_registry_client::SigningKey::generate();
    let other = specforge_registry_client::SigningKey::generate();
    let registry = serve(vec![
        Package::new("@sdk/greet", "0.1.0", greet_wasm()).signed_by(&other),
    ]);
    let dir = project_on(&registry);
    let mut known = specforge_registry_client::KnownKeys::default();
    known.pin("@sdk/greet", &pinned.key_id());
    specforge_registry_client::save_known_keys_at(&known_keys(dir.home()), &known).unwrap();

    let replies = session(
        &dir,
        dir.root(),
        ".",
        &[
            initialize(1),
            request(
                2,
                "tools/call",
                json!({"name": "specforge.add_extension", "arguments": {"specifiers": ["@sdk/greet@0.1.0"]}}),
            ),
        ],
        &[],
    );

    let reply = replies
        .iter()
        .find(|r| r["id"] == 2)
        .expect("a reply to the call");
    let error = parse_tool_content(reply);
    assert_eq!(error["diagnostic"]["code"], "R-TRUST-003", "{error}");
    let suggestion = error["diagnostic"]["suggestion"]
        .as_str()
        .unwrap_or_default();
    assert!(suggestion.contains("`specforge add`"), "{error}");
}
