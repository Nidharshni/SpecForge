//! Inference gaps over a served project: what covers a source item is what anchors it (ADR 0199).

use crate::support::*;
use serde_json::{Value, json};
use specforge_extension_sdk::prelude::*;
use specforge_test::prelude::*;

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

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "an item an anchor names for a declared entity is covered"
)]
fn an_item_an_anchor_names_for_a_declared_entity_is_covered() {
    let mut server = served(Some(ANCHORS), &[("src/lib.rs", LIB)]);
    let items = gap_items(&mut server);
    assert!(
        !items.contains(&("src/lib.rs".into(), "CompiledProject".into())),
        "{items:?}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "an anchored entity covers only the items its anchors name"
)]
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

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "an unusable anchors manifest refuses the gaps with E071"
)]
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

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "every item find_spec_for_source anchors to a declared entity is no gap"
)]
fn every_item_find_spec_for_source_anchors_to_a_declared_entity_is_no_gap() {
    let anchors = r#"{"version":1,"anchors":[
{"entity_id":"compiled_project","file":"./src/lib.rs","line":1,"symbol_name":"CompiledProject","item_kind":"struct","scanner":"s"},
{"entity_id":"compile_project","file":"src/lib.rs","line":2,"symbol_name":"compile_project","item_kind":"fn","scanner":"s"},
{"entity_id":"ghost","file":"src/lib.rs","line":3,"symbol_name":"Ghost","item_kind":"struct","scanner":"s"}]}"#;
    let lib = format!("{LIB}pub struct Ghost;\n");
    let mut server = served(Some(anchors), &[("src/lib.rs", lib.as_str())]);
    let reply = call_tool(
        &mut server,
        "specforge.find_spec_for_source",
        json!({"file_path": "src"}),
    );
    let anchored: Value = serde_json::from_str(&tool_text(&reply)).unwrap();
    let items = gap_items(&mut server);
    let mut declared = 0;
    for entity in anchored["entities"].as_array().unwrap() {
        let item = (
            "src/lib.rs".to_string(),
            entity["symbol_name"].as_str().unwrap().to_string(),
        );
        if entity.get("kind").is_some() {
            declared += 1;
            assert!(!items.contains(&item), "{item:?} in {items:?}");
        } else {
            assert!(items.contains(&item), "{item:?} not in {items:?}");
        }
    }
    assert_eq!(declared, 2, "{anchored}");
}

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "an item whose name is an unanchored entity's id is covered"
)]
fn an_item_whose_name_is_an_unanchored_entitys_id_is_covered() {
    let mut server = served(
        Some(ANCHORS),
        &[("src/lib.rs", LIB), ("src/util.rs", "pub fn helper() {}\n")],
    );
    assert_eq!(
        gap_items(&mut server),
        [],
        "helper has no anchor, so its name decides"
    );
}

#[specforge_test(
    behavior = "provide_mcp_infer_gaps_tool",
    verify = "an anchor to an entity the graph does not declare covers nothing"
)]
fn an_anchor_to_an_entity_the_graph_does_not_declare_covers_nothing() {
    let anchors = r#"{"version":1,"anchors":[
{"entity_id":"ghost","file":"src/other.rs","line":1,"symbol_name":"Other","item_kind":"struct","scanner":"s"}]}"#;
    let mut server = served(Some(anchors), &[("src/other.rs", "pub struct Other;\n")]);
    assert_eq!(
        gap_items(&mut server),
        [("src/other.rs".to_string(), "Other".to_string())]
    );
}
