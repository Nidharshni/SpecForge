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

const BEGIN: &str = "<!-- relations:begin (generated: SPECFORGE_BLESS=1 cargo nextest run -p specforge-cli -E 'test(the_quick_reference_relations_are_generated)'; never edit by hand) -->";
const END: &str = "<!-- relations:end -->";

/// One field of a builtin kind, as its pinned declaration says it.
struct Field {
    name: String,
    /// `identifier`, `enum: a, b` or `None` for the types the lists leave bare.
    note: Option<String>,
    required: bool,
    target: Option<String>,
    edge: Option<String>,
    inverse_of: Option<String>,
    derived: bool,
    /// The extension an enhancement field comes from.
    enhanced_by: Option<String>,
}

struct Kind {
    extension: String,
    keyword: String,
    description: String,
    fields: BTreeMap<String, Field>,
}

fn items(ext: &str, file: &str) -> Vec<serde_json::Value> {
    let path = repo_root()
        .join("crates/runtime/component/tests/declarations")
        .join(ext)
        .join(file);
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid describe JSON");
    doc["items"].as_array().cloned().unwrap_or_default()
}

fn field(value: &serde_json::Value, enhanced_by: Option<&str>) -> Field {
    let text = |key: &str| value[key].as_str().map(str::to_string);
    let note = match value["field_type"].as_str() {
        Some("identifier") => Some("identifier".to_string()),
        Some("enum") => Some(format!(
            "enum: {}",
            value["enum_values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => None,
    };
    let reference = value["field_type"]
        .as_str()
        .is_some_and(|t| t.starts_with("reference"));
    Field {
        name: text("name").expect("field name"),
        note,
        required: value["required"].as_bool().unwrap_or(false),
        target: text("target_kind").filter(|_| reference),
        edge: text("edge"),
        inverse_of: text("inverse_of"),
        derived: value.get("derived_from").is_some_and(|d| !d.is_null()),
        enhanced_by: enhanced_by.map(str::to_string),
    }
}

/// Every builtin kind (by extension, then keyword) with its own, shared and enhancement fields.
fn kinds() -> Vec<Kind> {
    let mut kinds: Vec<Kind> = Vec::new();
    let mut shared = Vec::new();
    for ext in EXTENSIONS {
        let name = format!("@specforge/{ext}");
        shared.extend(items(ext, "describe_shared_fields.json"));
        let mut own: Vec<Kind> = items(ext, "describe_entities.json")
            .iter()
            .map(|item| Kind {
                extension: name.clone(),
                keyword: item["keyword"].as_str().expect("keyword").to_string(),
                description: item["description"].as_str().unwrap_or_default().to_string(),
                fields: item["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| (f["name"].as_str().unwrap().to_string(), field(f, None)))
                    .collect(),
            })
            .collect();
        own.sort_by(|a, b| a.keyword.cmp(&b.keyword));
        kinds.extend(own);
    }
    for ext in EXTENSIONS {
        let name = format!("@specforge/{ext}");
        for item in items(ext, "describe_enhancements.json") {
            let target = item["target_kind"].as_str().expect("enhanced kind");
            let kind = kinds
                .iter_mut()
                .find(|k| k.keyword == target)
                .expect("an enhancement of a builtin kind");
            for f in item["fields"].as_array().into_iter().flatten() {
                let enhanced = (kind.extension != name).then_some(name.as_str());
                kind.fields
                    .insert(f["name"].as_str().unwrap().to_string(), field(f, enhanced));
            }
        }
    }
    for kind in &mut kinds {
        for f in &shared {
            kind.fields
                .entry(f["name"].as_str().unwrap().to_string())
                .or_insert_with(|| field(f, None));
        }
    }
    kinds
}

fn listed(field: &Field) -> String {
    match &field.note {
        Some(note) => format!("`{}` ({note})", field.name),
        None => format!("`{}`", field.name),
    }
}

/// The generated block of `docs/quick-reference.md`: every kind's fields and relations, then the
/// table of owning fields (ADR 0150).
fn render() -> String {
    let kinds = kinds();
    let mut out = String::new();
    let mut table = String::new();
    let mut written: BTreeSet<&str> = BTreeSet::new();
    for kind in &kinds {
        let plain = |required: bool| -> String {
            kind.fields
                .values()
                .filter(|f| f.name != "title" && f.required == required)
                .map(listed)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let writes: Vec<String> = kind
            .fields
            .values()
            .filter_map(|f| {
                let target = f.target.as_deref()?;
                let edge = f
                    .edge
                    .as_deref()
                    .map(|e| format!(" (`{e}`)"))
                    .unwrap_or_default();
                let mut marks = String::new();
                if let Some(ext) = &f.enhanced_by {
                    marks += &format!(" (from {ext})");
                }
                if f.derived {
                    marks += " (derived)";
                }
                Some(format!("`{}` → {target}{edge}{marks}", f.name))
            })
            .collect();
        let mut other_side = Vec::new();
        for owner in kinds.iter().filter(|o| o.keyword != kind.keyword) {
            for f in owner.fields.values() {
                if f.target.as_deref() == Some(kind.keyword.as_str()) {
                    let mut notes = Vec::new();
                    if let Some(edge) = &f.edge {
                        notes.push(format!("`{edge}`"));
                    }
                    if let Some(read) = &f.inverse_of {
                        notes.push(format!("read here as `{read}`"));
                    }
                    let notes = if notes.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", notes.join(", "))
                    };
                    other_side.push(format!("{} `{}`{notes}", owner.keyword, f.name));
                }
            }
        }
        out += &format!(
            "### {}\n> {} — {}\n\n",
            kind.keyword, kind.extension, kind.description
        );
        for (label, list) in [("Required", plain(true)), ("Optional", plain(false))] {
            if !list.is_empty() {
                out += &format!("{label}: {list}\n");
            }
        }
        if !writes.is_empty() {
            out += &format!("Writes: {}\n", writes.join(", "));
        }
        if !other_side.is_empty() {
            out += &format!(
                "Written on the other side (derived, never a field here): {}\n",
                other_side.join(", ")
            );
        }
        out += "\n";
        for f in kind.fields.values() {
            if let Some(target) = &f.target {
                let edge = f.edge.as_deref().unwrap_or("");
                let from = f.enhanced_by.as_deref().unwrap_or(&kind.extension);
                table += &format!(
                    "| `{}.{}` | {} | {target} | {} | {from} |\n",
                    kind.keyword,
                    f.name,
                    kind.keyword,
                    if edge.is_empty() {
                        String::new()
                    } else {
                        format!("`{edge}`")
                    },
                );
                written.extend(f.edge.as_deref());
            }
        }
    }
    let unwritten: BTreeSet<String> = EXTENSIONS
        .iter()
        .flat_map(|ext| items(ext, "describe_edges.json"))
        .filter_map(|e| e["label"].as_str().map(str::to_string))
        .filter(|label| !written.contains(label.as_str()))
        .collect();
    out += "### Relations\n\nA relation is written on its owning field only; the other side is derived (ADR 0150).\n\n";
    out += "| Owning field | From | To | Edge type | Extension |\n|---|---|---|---|---|\n";
    out += &table;
    out += &format!(
        "\nEdge types no field writes: {}.\n",
        unwritten
            .iter()
            .map(|e| format!("`{e}`"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    out
}

#[test]
fn the_quick_reference_relations_are_generated() {
    let path = repo_root().join("docs/quick-reference.md");
    let text = std::fs::read_to_string(&path).expect("docs/quick-reference.md");
    let (begin, end) = (
        text.find("<!-- relations:begin").expect("the begin marker"),
        text.find(END).expect("the end marker"),
    );
    let body = begin + text[begin..].find('\n').unwrap() + 1;
    let expected = render();
    if std::env::var_os("SPECFORGE_BLESS").is_some() {
        let rewritten = format!("{}{BEGIN}\n{expected}{}", &text[..begin], &text[end..]);
        std::fs::write(&path, rewritten).unwrap();
        return;
    }
    assert!(
        text[body..end] == expected,
        "the generated block of docs/quick-reference.md was edited by hand or is stale: run with SPECFORGE_BLESS=1"
    );
}

#[test]
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
