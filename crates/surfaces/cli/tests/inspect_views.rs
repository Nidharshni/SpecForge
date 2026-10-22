//! Inspect on every surface (architecture plan 2026-10-06, 07).
//!
//! MCP `specforge.inspect`, the MCP context prompt and the LSP hover each
//! say what one entity is. The `*_today` tests pin what each answers on two
//! small projects, `fixtures/coverage/fx1` and `fixtures/navigation/cyc`, as
//! insta snapshots (`tests/snapshots/tests__inspect_views__*.snap`). They
//! prove no spec obligation (they pin current behavior, bugs included), so
//! they carry no `specforge_test` link; a change that alters what they pin
//! re-blesses the snapshot in the same commit, where the diff shows it.
//!
//! `hover_and_inspect_agree_on_every_entity` is the linked parity test: on
//! every entity of both projects the hover and inspect report the same
//! standing, coverage, references and diagnostics.
//!
//! cyc holds two behaviors, `alpha` and `beta`, that depend on each other
//! (a reference cycle, W061, which names both in its data).

use serde_json::{Value, json};
use specforge_mcp::testing::Served;
use std::path::Path;

use crate::coverage_corpus::project;
use crate::parity::normalized;
use crate::support::TestProject;
use crate::support::lsp::Client;
use crate::support::pairing::{mcp_calls, served};

/// The entities of fx1 the views are pinned on.
const FX1: [&str; 6] = [
    "login",
    "logout",
    "Status",
    "signin",
    "Payload",
    "no_lost_login",
];

/// The entities of cyc the views are pinned on.
const CYC: [&str; 2] = ["alpha", "beta"];

/// A scratch copy of `fixtures/navigation/cyc` (its spec root is the
/// project root).
fn cyc() -> TestProject {
    TestProject::copy_of(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/navigation/cyc"),
    )
}

fn inspect(entity_id: &str) -> Value {
    json!({"name": "specforge.inspect", "arguments": {"entity_id": entity_id}})
}

/// The 0-based position of `id`'s declaration name in `text`: the line
/// whose second word is `id`, one character into the name.
fn declaration(text: &str, id: &str) -> (u32, u32) {
    text.lines()
        .enumerate()
        .find_map(|(line, l)| {
            (l.split_whitespace().nth(1) == Some(id))
                .then(|| (line as u32, (l.find(id).unwrap() + 1) as u32))
        })
        .unwrap_or_else(|| panic!("no declaration of {id}"))
}

/// The hover markdown at each position of `file` (relative to `root`), in
/// order, from the real LSP backend serving `root`; `"null"` for no hover.
fn hovers(root: &Path, file: &str, positions: &[(u32, u32)]) -> Vec<String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut client = Client::start();
        client.open_workspace(root).await;
        let path = root.join(file);
        client.open(&path).await;
        let uri = tower_lsp::lsp_types::Url::from_file_path(&path)
            .unwrap()
            .to_string();
        let mut out = Vec::new();
        for (line, character) in positions {
            let response = client
                .request(
                    "textDocument/hover",
                    json!({"textDocument": {"uri": uri},
                           "position": {"line": line, "character": character}}),
                )
                .await;
            let result = &response["result"];
            out.push(match result["contents"]["value"].as_str() {
                Some(markdown) => markdown.to_string(),
                None => result.to_string(),
            });
        }
        out
    })
}

#[test]
fn inspect_today() {
    let mut fx1 = served(project("fx1"));
    let root_buf = fx1.root().to_path_buf();
    let root = root_buf.as_path();
    let mut calls: Vec<Value> = FX1.iter().map(|id| inspect(id)).collect();
    calls.push(inspect("ghost"));
    let results = mcp_calls(&mut fx1, &calls);
    for (id, result) in FX1.iter().chain(["ghost"].iter()).zip(&results) {
        insta::assert_snapshot!(format!("inspect_fx1_{id}"), normalized(result, root));
    }

    let mut cyc = served(cyc());
    let root_buf = cyc.root().to_path_buf();
    let root = root_buf.as_path();
    let calls: Vec<Value> = CYC.iter().map(|id| inspect(id)).collect();
    for (id, result) in CYC.iter().zip(mcp_calls(&mut cyc, &calls)) {
        insta::assert_snapshot!(format!("inspect_cyc_{id}"), normalized(&result, root));
    }
}

#[test]
fn context_prompt_today() {
    let mut fx1 = served(project("fx1"));
    let root_buf = fx1.root().to_path_buf();
    let root = root_buf.as_path();
    let calls = [json!({"method": "prompts/get", "params": {
        "name": "specforge://prompts/context", "arguments": {"entity_id": "login"}}})];
    let result = mcp_calls(&mut fx1, &calls).pop().unwrap();
    insta::assert_snapshot!("context_fx1_login", normalized(&result, root));
}

#[test]
fn hover_today() {
    let fx1 = served(project("fx1"));
    let root_buf = fx1.root().to_path_buf();
    let root = root_buf.as_path();
    let text = std::fs::read_to_string(root.join("spec/main.spec")).unwrap();
    let mut positions: Vec<(u32, u32)> = FX1.iter().map(|id| declaration(&text, id)).collect();
    // `signin` inside `login`'s `features [signin]`.
    positions.push((2, 13));
    let markdown = hovers(root, "spec/main.spec", &positions);
    let names = FX1
        .iter()
        .map(|id| format!("hover_fx1_{id}"))
        .chain(["hover_fx1_signin_reference".to_string()]);
    for (name, markdown) in names.zip(markdown) {
        insta::assert_snapshot!(name, markdown);
    }

    let cyc = cyc();
    let root = cyc.root();
    let markdown = hovers(root, "a.spec", &[(0, 10), (3, 10)]);
    for (id, markdown) in CYC.iter().zip(markdown) {
        insta::assert_snapshot!(format!("hover_cyc_{id}"), markdown);
    }
}

// ── Parity ──────────────────────────────────────────────────────────────

/// The code a hover line names: the text between its first `**` pair
/// (`**W006** · …`, `- [**I048**](…) …`, `- **X900** …`).
fn code_of(line: &str) -> String {
    let start = line.find("**").unwrap() + 2;
    let end = start + line[start..].find("**").unwrap();
    line[start..end].to_string()
}

/// The ids a hover reference line lists (`- \`field\` → a, b`, `- kind via
/// \`field\`: a, b`).
fn listed_ids(line: &str) -> Vec<String> {
    let (_, ids) = line
        .split_once(" → ")
        .or_else(|| line.split_once("`: "))
        .unwrap_or_else(|| panic!("not a reference line: {line}"));
    ids.split(", ").map(str::to_string).collect()
}

/// A reference section: its count and the distinct ids it lists, sorted.
fn references(section: Option<&&str>) -> (u64, Vec<String>) {
    let Some(section) = section else {
        return (0, Vec::new());
    };
    let mut lines = section.lines();
    let heading = lines.next().unwrap();
    let count = heading
        .rsplit_once("*(")
        .and_then(|(_, n)| n.strip_suffix(")*"))
        .unwrap()
        .parse()
        .unwrap();
    let mut ids: Vec<String> = lines.flat_map(listed_ids).collect();
    ids.sort();
    ids.dedup();
    (count, ids)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not an array: {value}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

/// Every entity of the project at `root` (its spec in `file`): its hover at
/// its declaration name and `specforge.inspect` report the same facts.
fn assert_hover_and_inspect_agree(served: &mut Served, file: &str) {
    let root_buf = served.root().to_path_buf();
    let root = root_buf.as_path();
    let listed = mcp_calls(
        served,
        &[json!({"name": "specforge.list", "arguments": {}})],
    )
    .pop()
    .unwrap();
    let ids: Vec<String> = listed["entities"]
        .as_array()
        .unwrap_or_else(|| panic!("specforge.list: {listed}"))
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!ids.is_empty());
    let calls: Vec<Value> = ids.iter().map(|id| inspect(id)).collect();
    let inspected = mcp_calls(served, &calls);
    let text = std::fs::read_to_string(root.join(file)).unwrap();
    let positions: Vec<(u32, u32)> = ids.iter().map(|id| declaration(&text, id)).collect();
    let hovered = hovers(root, file, &positions);

    for ((id, inspect), hover) in ids.iter().zip(&inspected).zip(&hovered) {
        let sections: Vec<&str> = hover.split("\n\n---\n\n").collect();
        let header_at = sections
            .iter()
            .position(|s| {
                s.starts_with(&format!("**{}** `{id}`", inspect["kind"].as_str().unwrap()))
            })
            .unwrap_or_else(|| panic!("{id}: no entity header in\n{hover}"));
        let header = sections[header_at];
        let section = |heading: &str| sections.iter().find(|s| s.starts_with(heading));

        // The testable badge is inspect's testable.
        assert_eq!(
            header.contains("`testable`"),
            inspect["testable"] == true,
            "{id}: testable\n{hover}"
        );

        // Every entity has a Coverage line: inspect's status first, then the
        // reason an exempt entity is exempt (obligated).
        let line = section("**Coverage**")
            .unwrap_or_else(|| panic!("{id}: a hover without coverage\n{hover}"));
        let status = inspect["coverage_status"].as_str().unwrap();
        let start = format!("**Coverage** · `{status}`");
        assert!(line.starts_with(&start), "{id}: {line} vs {status}");
        if status == "exempt" {
            let reason = if inspect["obligated"] == true {
                "it owes none (a union or an exempting field)"
            } else {
                "its kind need not declare obligations"
            };
            assert_eq!(*line, format!("{start} · {reason}"), "{id}");
        }

        // The references: each direction's ids, and their counts together.
        let (out_count, out_ids) = references(section("**Refers to**"));
        let (in_count, in_ids) = references(section("**Referenced by**"));
        assert_eq!(out_ids, strings(&inspect["refers_to"]), "{id}: refers to");
        assert_eq!(
            in_ids,
            strings(&inspect["referenced_by"]),
            "{id}: referenced by"
        );
        assert_eq!(
            in_count + out_count,
            (strings(&inspect["refers_to"]).len() + strings(&inspect["referenced_by"]).len())
                as u64,
            "{id}: reference counts"
        );

        // The members (ADR 0222): the Methods lines are inspect's methods, and a top-level
        // field's line ends with its annotations as inspect lists them.
        let methods = inspect["methods"].as_array().map_or(0, Vec::len);
        let listed = section("**Methods**").map_or(0, |s| s.lines().count() - 1);
        assert_eq!(listed, methods, "{id}: methods\n{hover}");
        for (path, annotations) in inspect["annotations"].as_object().into_iter().flatten() {
            if path.contains('.') {
                continue;
            }
            let spelled: String = annotations
                .as_array()
                .unwrap()
                .iter()
                .map(|a| match a["value"].as_str() {
                    Some(value) => format!(" @{} {value:?}", a["name"].as_str().unwrap()),
                    None => format!(" @{}", a["name"].as_str().unwrap()),
                })
                .collect();
            let line = hover
                .lines()
                .find(|l| l.starts_with(&format!("- `{path}` = ")))
                .unwrap_or_else(|| panic!("{id}: no line for `{path}`\n{hover}"));
            assert!(line.ends_with(&spelled), "{id}: {line} vs {spelled}");
        }

        // The diagnostics: those under the cursor and those listed, as a
        // multiset, are inspect's.
        let mut shown: Vec<String> = sections[..header_at].iter().map(|s| code_of(s)).collect();
        if let Some(listed) = section("**Diagnostics**") {
            shown.extend(listed.lines().skip(1).map(code_of));
        }
        shown.sort();
        let mut reported: Vec<String> = inspect["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect();
        reported.sort();
        assert_eq!(shown, reported, "{id}: diagnostics\n{hover}");
    }
}

#[specforge_test_macros::test(
    behavior = "read_views_over_the_project_view",
    verify = "specforge.inspect and the LSP hover report the same facts for an entity"
)]
fn hover_and_inspect_agree_on_every_entity() {
    assert_hover_and_inspect_agree(&mut served(project("fx1")), "spec/main.spec");
    assert_hover_and_inspect_agree(&mut served(cyc()), "a.spec");
}
