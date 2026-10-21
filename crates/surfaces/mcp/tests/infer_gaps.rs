//! Inference gaps over a served project: what covers a source item is what anchors it (ADR 0199).

use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::*;

/// The rust analyzer: one item per `pub <keyword> <Name>` line.
fn rust(c: &mut ContributionsBuilder) {
    c.analyzer("rust", |a| {
        a.file_extensions(&[".rs"]).scan(|request| ScanResponse {
            items: request
                .content
                .lines()
                .enumerate()
                .filter_map(|(i, line)| {
                    let mut words = line.strip_prefix("pub ")?.split_whitespace();
                    let item_kind = words.next()?.to_string();
                    let name: String = words
                        .next()?
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    Some(ScannedItem {
                        name,
                        item_kind,
                        line: i + 1,
                        visibility: None,
                        signature: None,
                    })
                })
                .collect(),
            language: None,
        });
    });
}

const SPEC: &str = concat!(
    "behavior compiled_project \"C\" {\n    contract \"c\"\n}\n",
    "behavior compile_project \"P\" {\n    contract \"c\"\n}\n",
    "behavior helper \"H\" {\n    contract \"c\"\n}\n",
);

const ANCHORS: &str = r#"{"version":1,"anchors":[
{"entity_id":"compiled_project","file":"src/lib.rs","line":1,"symbol_name":"CompiledProject","item_kind":"struct","scanner":"@specforge/rust"},
{"entity_id":"compile_project","file":"src/lib.rs","line":2,"symbol_name":"compile_project","item_kind":"fn","scanner":"@specforge/rust"}]}"#;

const LIB: &str = "pub struct CompiledProject;\npub fn compile_project() {}\n";
const OTHER: &str = "pub fn compile_project() {}\n";

fn served(anchors: Option<&str>, files: &[(&str, &str)]) -> Served {
    let mut project = TestProject::new().file("test.spec", SPEC).files(files);
    if let Some(anchors) = anchors {
        project = project.file("specforge-anchors.json", anchors);
    }
    project.serve(&[
        TestExtension::software(),
        TestExtension::named("@specforge/rust").declaring(rust),
    ])
}

fn gaps(server: &mut Served) -> Value {
    let reply = call_tool(server, "specforge.infer_gaps", json!({}));
    serde_json::from_str(&tool_text(&reply)).unwrap()
}

/// `(file, name)` of every listed gap.
fn gap_items(server: &mut Served) -> Vec<(String, String)> {
    let gaps = gaps(server);
    gaps["by_directory"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["items"].as_array().unwrap())
        .map(|i| {
            (
                i["file"].as_str().unwrap().to_string(),
                i["name"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
#[ignore = "flipped by T3"]
fn an_item_an_anchor_names_for_a_declared_entity_is_covered() {
    let mut server = served(Some(ANCHORS), &[("src/lib.rs", LIB)]);
    let items = gap_items(&mut server);
    assert!(
        !items.contains(&("src/lib.rs".into(), "CompiledProject".into())),
        "{items:?}"
    );
}

#[test]
#[ignore = "flipped by T3"]
fn an_anchored_entity_covers_only_the_items_its_anchors_name() {
    let mut server = served(
        Some(ANCHORS),
        &[("src/lib.rs", LIB), ("src/other.rs", OTHER)],
    );
    let items = gap_items(&mut server);
    assert!(
        items.contains(&("src/other.rs".into(), "compile_project".into())),
        "{items:?}"
    );
    let document = gaps(&mut server);
    assert_eq!(document["covered_items"], 2, "{document}");
    assert_eq!(document["total_pub_items"], 3, "{document}");
}

#[test]
#[ignore = "flipped by T3"]
fn an_unusable_anchors_manifest_refuses_the_gaps() {
    let mut server = served(
        Some(r#"{"version":1,"anchors":[{"entity_id":"x"}]}"#),
        &[("src/lib.rs", LIB)],
    );
    let reply = call_tool(&mut server, "specforge.infer_gaps", json!({}));
    let error = crate::tool_errors::mcp_error(&reply);
    assert_eq!(error["code"], "schema_mismatch", "{error}");
    assert_eq!(error["diagnostic"]["code"], "E071", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("failed to parse specforge-anchors.json: missing field `file`"),
        "{error}"
    );
}
