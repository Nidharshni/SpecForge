//! The `specforge:bridge@0.1.0` world is written three times, and this test holds the
//! copies to one world:
//!
//! - the host's `crates/runtime/component/wit/bridge.wit` (read by `bindgen!`);
//! - the `inline:` literal in the SDK's `component_guest!` (`extension-sdk/src/guest.rs`):
//!   wit-bindgen 0.30 takes `inline:` only as a string literal, and `path:` there would
//!   resolve against the extension's crate, not the SDK's;
//! - the SDK-free fixture's `fixtures/test-component/wit/bridge.wit`, which leaves the
//!   SDK out on purpose.
//!
//! A change to one is a change to all three, then a re-vendor (`docs/agents/revendor.md`).
//! Comments, blank lines and indentation may differ; everything else may not.

use std::path::Path;

const HOST: &str = "crates/runtime/component/wit/bridge.wit";
const SDK: &str = "crates/runtime/extension-sdk/src/guest.rs";
const FIXTURE: &str = "fixtures/test-component/wit/bridge.wit";

fn read(rel: &str) -> String {
    let path = specforge_repo_root::repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", Path::new(&path).display()))
}

/// The WIT text with comments, blank lines and indentation dropped.
fn world(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect()
}

/// The `inline: r#"..."#` literal of `component_guest!`.
fn sdk_literal(guest_rs: &str) -> &str {
    let start = guest_rs
        .find("inline: r#\"")
        .expect("component_guest! has an `inline: r#\"` WIT literal")
        + "inline: r#\"".len();
    let len = guest_rs[start..]
        .find("\"#")
        .expect("component_guest!'s WIT literal ends with `\"#`");
    &guest_rs[start..start + len]
}

#[test]
fn the_three_bridge_worlds_are_one() {
    let host = read(HOST);
    let sdk = read(SDK);
    let fixture = read(FIXTURE);
    let host_world = world(&host);
    assert!(
        host_world.iter().any(|l| l.starts_with("export call:")),
        "{HOST} exports `call`"
    );
    assert_eq!(
        world(sdk_literal(&sdk)),
        host_world,
        "the WIT literal in {SDK} differs from {HOST}: the three copies change together"
    );
    assert_eq!(
        world(&fixture),
        host_world,
        "{FIXTURE} differs from {HOST}: the three copies change together"
    );
}
