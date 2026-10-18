//! The relations the docs state follow the builtin declarations (ADR 0150): the quick reference
//! generates them, and an entity page lists only fields its kind registers.

use std::collections::{BTreeMap, BTreeSet};

use specforge_repo_root::repo_root;

const EXTENSIONS: [&str; 4] = ["product", "software", "governance", "formal"];

/// Every field name each builtin kind registers (its own fields and the enhancements') from the
/// pinned wire declarations, plus the shared fields every kind has.
fn registered_fields() -> BTreeMap<String, BTreeSet<String>> {
    let pinned = repo_root().join("crates/runtime/component/tests/declarations");
    let read = |ext: &str, file: &str| -> Option<serde_json::Value> {
        let raw = std::fs::read_to_string(pinned.join(ext).join(file)).ok()?;
        Some(serde_json::from_str(&raw).expect("valid describe JSON"))
    };
    let names = |fields: &serde_json::Value| -> Vec<String> {
        fields
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f["name"].as_str().map(str::to_string))
            .collect()
    };
    let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut shared = BTreeSet::new();
    for ext in EXTENSIONS {
        for item in read(ext, "describe_entities.json").map_or(vec![], |d| {
            d["items"].as_array().cloned().unwrap_or_default()
        }) {
            let kind = item["keyword"].as_str().expect("kind keyword").to_string();
            kinds
                .entry(kind)
                .or_default()
                .extend(names(&item["fields"]));
        }
        for item in read(ext, "describe_enhancements.json").map_or(vec![], |d| {
            d["items"].as_array().cloned().unwrap_or_default()
        }) {
            let kind = item["target_kind"]
                .as_str()
                .expect("enhanced kind")
                .to_string();
            kinds
                .entry(kind)
                .or_default()
                .extend(names(&item["fields"]));
        }
        if let Some(d) = read(ext, "describe_shared_fields.json") {
            shared.extend(names(&d["items"]));
        }
    }
    for fields in kinds.values_mut() {
        fields.extend(shared.iter().cloned());
    }
    kinds
}

#[test]
#[ignore = "flipped by T12"]
fn the_quick_reference_relations_are_generated() {
    let text = std::fs::read_to_string(repo_root().join("docs/quick-reference.md"))
        .expect("docs/quick-reference.md");
    assert!(
        text.contains("<!-- relations:begin") && text.contains("<!-- relations:end -->"),
        "the entities and relations of the quick reference are not generated"
    );
}

#[test]
#[ignore = "flipped by T13"]
fn entity_pages_list_only_declared_reference_fields() {
    let registered = registered_fields();
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(repo_root().join("docs/entities")).expect("docs/entities") {
        let path = entry.unwrap().path();
        let Some(page) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let kind = page.replace('-', "_");
        let Some(fields) = registered.get(&kind) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).unwrap();
        for (index, line) in text.lines().enumerate() {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let [_, field, ty, ..] = cells.as_slice() else {
                continue;
            };
            let name = field.trim_matches('`');
            if field.starts_with('`')
                && (ty.starts_with("reference") || ty.starts_with("EntityId"))
                && !fields.contains(name)
            {
                problems.push(format!(
                    "docs/entities/{page}.md:{}: `{name}` is no field of `{kind}`",
                    index + 1
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
