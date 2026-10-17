//! Navigation parity harness (architecture plan 05, T1).
//!
//! The LSP and MCP answer the same navigation questions about the same
//! project: where an entity is declared, what references it, which
//! entities match a text, which diagnostics are about an entity, and how a
//! diagnostic is fixed. This harness asks each question of both surfaces,
//! in process (the LSP `Backend` over memory streams, `McpServer` through
//! `handle_message`), over the projects under `tests/fixtures/navigation/`.
//!
//! Answers are normalized to keys: a location is `"file L:C-L:C"`, 1-based
//! lines and columns, end exclusive (the parser's `SourceSpan`); the LSP's
//! 0-based UTF-16 positions convert to that (the fixtures are ASCII). Each
//! case names the answer both surfaces should give ([`Case::target`]).
//! Where a surface answers otherwise today, the disagreement is a row of
//! [`EXPECTED_DIVERGENCES`] holding what it answers ([`Divergence::today`]).
//! A case passes only when every surface answers exactly its row's `today`,
//! or, without a row, the target: a ticket that closes a divergence fails
//! this test until it deletes the row, so a step can only turn green on
//! purpose.
//!
//! The per-case tests characterize and are not linked. The plain tests at
//! the end pin behaviour the plan keeps.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use specforge_test::prelude::*;
use tower_lsp::lsp_types::Url;

use crate::support::TestProject;
use crate::support::lsp::Client;

// ── Fixtures ────────────────────────────────────────────────────────────

/// A fixture copied to a temporary directory.
struct Project {
    _dir: TestProject,
    /// Canonical project root, which is also the spec root.
    root: PathBuf,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/navigation")
}

fn project(fixture: &str) -> Project {
    let dir = TestProject::copy_of(fixtures_dir().join(fixture));
    let root = fs::canonicalize(dir.root()).unwrap();
    Project { _dir: dir, root }
}

impl Project {
    fn uri(&self, file: &str) -> String {
        Url::from_file_path(self.root.join(file))
            .unwrap()
            .to_string()
    }

    /// The `.spec` files of the project, sorted.
    fn spec_files(&self) -> Vec<String> {
        let mut files: Vec<String> = fs::read_dir(&self.root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".spec"))
            .collect();
        files.sort();
        files
    }

    /// A URI of this project, relative to its root.
    fn relative(&self, uri: &str) -> String {
        let path = Url::parse(uri).unwrap().to_file_path().unwrap();
        path.strip_prefix(&self.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned()
    }
}

// ── MCP ─────────────────────────────────────────────────────────────────

/// The result of MCP tool `name` with `arguments`, served from the
/// project: its JSON payload, or, for a failed call, the whole result.
fn mcp_tool(project: &Project, name: &str, arguments: Value) -> Value {
    mcp_result(project, name, arguments).0
}

/// [`mcp_tool`], and whether the call failed (`isError`).
fn mcp_result(project: &Project, name: &str, arguments: Value) -> (Value, bool) {
    let mut server = specforge_mcp::McpServer::new();
    let mut call = |method: &str, params: Value| -> Value {
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let resp = server.handle_message(&req.to_string()).unwrap();
        serde_json::from_str(&resp).unwrap()
    };
    call(
        "initialize",
        json!({"projectRoot": project.root.to_str().unwrap()}),
    );
    let resp = call("tools/call", json!({"name": name, "arguments": arguments}));
    let result = &resp["result"];
    let failed = result["isError"] == true;
    if failed {
        return (result.clone(), true);
    }
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{name} returned no text: {resp}"));
    let reply: Value = serde_json::from_str(text).unwrap();
    // The array tools answer an object holding the array.
    let key = match name {
        "specforge.search" => "results",
        "specforge.outline" => "entries",
        "specforge.suggest_fixes" => "fixes",
        _ => return (reply, false),
    };
    (reply.get(key).cloned().unwrap_or(reply), false)
}

// ── LSP ─────────────────────────────────────────────────────────────────

/// One LSP request, its params built from the project's URIs.
type Request = (&'static str, Value);

/// The results of `requests`, sent in order to an LSP serving the project
/// after every `.spec` file is opened, from a client declaring
/// `capabilities`.
fn lsp_requests(project: &Project, capabilities: Value, requests: Vec<Request>) -> Vec<Value> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut client = Client::start();
        client
            .open_workspace_with(&project.root, capabilities)
            .await;
        for file in project.spec_files() {
            client.open(&project.root.join(file)).await;
        }
        let mut results = Vec::new();
        for (method, params) in requests {
            let response = client.request(method, params).await;
            results.push(response["result"].clone());
        }
        results
    })
}

/// The result of one request ([`lsp_requests`]) from a client declaring no
/// capabilities.
fn lsp_request(project: &Project, method: &'static str, params: Value) -> Value {
    lsp_requests(project, json!({}), vec![(method, params)])
        .pop()
        .unwrap()
}

fn position(project: &Project, file: &str, line: u32, character: u32) -> Value {
    json!({
        "textDocument": {"uri": project.uri(file)},
        "position": {"line": line, "character": character},
    })
}

fn references_params(
    project: &Project,
    file: &str,
    line: u32,
    character: u32,
    include_declaration: bool,
) -> Value {
    let mut params = position(project, file, line, character);
    params["context"] = json!({"includeDeclaration": include_declaration});
    params
}

// ── Normalized answers ──────────────────────────────────────────────────

/// `"file L:C-L:C"`, 1-based, end exclusive.
fn span_key(file: &str, start: (u64, u64), end: (u64, u64)) -> String {
    format!("{file} {}:{}-{}:{}", start.0, start.1, end.0, end.1)
}

/// An LSP range (0-based), as a 1-based key in `file`.
fn range_key(file: &str, range: &Value) -> String {
    let at = |p: &Value| {
        (
            p["line"].as_u64().unwrap() + 1,
            p["character"].as_u64().unwrap() + 1,
        )
    };
    span_key(file, at(&range["start"]), at(&range["end"]))
}

/// An LSP `Location` as a key.
fn location_key(project: &Project, location: &Value) -> String {
    range_key(
        &project.relative(location["uri"].as_str().unwrap()),
        &location["range"],
    )
}

/// An MCP `SourceSpan` object (`file`, 1-based lines and columns) as a key.
fn mcp_span_key(span: &Value) -> String {
    let n = |k: &str| span[k].as_u64().unwrap();
    span_key(
        span["file"].as_str().unwrap(),
        (n("start_line"), n("start_col")),
        (n("end_line"), n("end_col")),
    )
}

/// The keys of an LSP `Location[]` result (`null` is none), sorted.
fn lsp_locations(project: &Project, result: &Value) -> Vec<String> {
    let mut keys: Vec<String> = result
        .as_array()
        .map(|locations| locations.iter().map(|l| location_key(project, l)).collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// The keys of MCP `find_references`: each location's span, then its
/// `field` (`-` when it has none).
fn mcp_references(result: &Value) -> Vec<String> {
    let mut keys: Vec<String> = result["locations"]
        .as_array()
        .unwrap_or_else(|| panic!("no locations in {result}"))
        .iter()
        .map(|l| {
            let field = l["field"].as_str().unwrap_or("-");
            format!("{} {field}", mcp_span_key(&l["source_span"]))
        })
        .collect();
    keys.sort();
    keys
}

/// The ids of MCP `search` results, in order.
fn search_ids(result: &Value) -> Vec<String> {
    result
        .as_array()
        .unwrap_or_else(|| panic!("search returned no list: {result}"))
        .iter()
        .map(|r| r["entity_id"].as_str().unwrap().to_string())
        .collect()
}

/// MCP `search` results as `"id match_field score"` (`-` without a
/// `match_field`; the score to 3 decimals), in order.
fn search_keys(result: &Value) -> Vec<String> {
    result
        .as_array()
        .unwrap_or_else(|| panic!("search returned no list: {result}"))
        .iter()
        .map(|r| {
            format!(
                "{} {} {:.3}",
                r["entity_id"].as_str().unwrap(),
                r["match_field"].as_str().unwrap_or("-"),
                r["score"].as_f64().unwrap()
            )
        })
        .collect()
}

/// An LSP `WorkspaceEdit`'s edits as `"file L:C-L:C new_text"`, sorted.
fn workspace_edit_keys(project: &Project, edit: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(changes) = edit["changes"].as_object() {
        for (uri, edits) in changes {
            let file = project.relative(uri);
            for e in edits.as_array().unwrap() {
                keys.push(format!(
                    "{} {}",
                    range_key(&file, &e["range"]),
                    e["newText"].as_str().unwrap().escape_debug()
                ));
            }
        }
    }
    keys.sort();
    keys
}

/// MCP rename edits (1-based line, 0-based byte columns) as 1-based keys,
/// sorted.
fn mcp_rename_keys(result: &Value) -> Vec<String> {
    let mut keys: Vec<String> = result["edits"]
        .as_array()
        .unwrap_or_else(|| panic!("no edits in {result}"))
        .iter()
        .map(|e| {
            let line = e["line"].as_u64().unwrap();
            format!(
                "{} {}",
                span_key(
                    e["file"].as_str().unwrap(),
                    (line, e["start_col"].as_u64().unwrap() + 1),
                    (line, e["end_col"].as_u64().unwrap() + 1),
                ),
                e["new_text"].as_str().unwrap()
            )
        })
        .collect();
    keys.sort();
    keys
}

/// LSP code actions as `"title kind edit…"`, each edit
/// `"file L:C-L:C new_text"`, sorted.
fn lsp_actions(project: &Project, result: &Value) -> Vec<String> {
    let mut keys: Vec<String> = result
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            let edits = workspace_edit_keys(project, &a["edit"]);
            format!(
                "{} {} {}",
                a["title"].as_str().unwrap(),
                a["kind"].as_str().unwrap_or("-"),
                if edits.is_empty() {
                    "-".to_string()
                } else {
                    edits.join(" | ")
                }
            )
        })
        .collect();
    keys.sort();
    keys
}

/// MCP fixes as `"title kind edit…"`, each edit `"file L:C-L:C new_text"`
/// (its `range` a `SourceSpan` without a file), sorted.
fn mcp_fixes(result: &Value) -> Vec<String> {
    let mut keys: Vec<String> = result
        .as_array()
        .unwrap_or_else(|| panic!("suggest_fixes returned no list: {result}"))
        .iter()
        .map(|f| {
            let edits: Vec<String> = f["edits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    let mut range = e["range"].clone();
                    range["file"] = e["file_path"].clone();
                    format!(
                        "{} {}",
                        mcp_span_key(&range),
                        e["new_text"].as_str().unwrap().escape_debug()
                    )
                })
                .collect();
            format!(
                "{} {} {}",
                f["title"].as_str().unwrap(),
                f["kind"].as_str().unwrap_or("-"),
                if edits.is_empty() {
                    "-".to_string()
                } else {
                    edits.join(" | ")
                }
            )
        })
        .collect();
    keys.sort();
    keys
}

/// An LSP `documentSymbol` result as an outline: one line per symbol,
/// `"name kind L:C-L:C"` then `" @L:C-L:C"` for its selection when it has
/// one; a nested symbol's line is indented under its parent's. A flat
/// `SymbolInformation` list has no selection and no children.
fn lsp_outline(project: &Project, file: &str, result: &Value) -> Vec<String> {
    fn walk(file: &str, symbols: &[Value], depth: usize, out: &mut Vec<String>) {
        for s in symbols {
            let (range, selection) = match s.get("location") {
                Some(location) => (range_key(file, &location["range"]), None),
                None => (
                    range_key(file, &s["range"]),
                    Some(range_key(file, &s["selectionRange"])),
                ),
            };
            let range = range.strip_prefix(&format!("{file} ")).unwrap().to_string();
            let mut line = format!(
                "{}{} {} {range}",
                "  ".repeat(depth),
                s["name"].as_str().unwrap(),
                symbol_kind(s)
            );
            if let Some(selection) = selection {
                line.push_str(&format!(
                    " @{}",
                    selection.strip_prefix(&format!("{file} ")).unwrap()
                ));
            }
            out.push(line);
            if let Some(children) = s["children"].as_array() {
                walk(file, children, depth + 1, out);
            }
        }
    }
    let _ = project;
    let mut out = Vec::new();
    walk(
        file,
        result.as_array().map_or(&[][..], Vec::as_slice),
        0,
        &mut out,
    );
    out
}

/// The entity kind an LSP symbol names: a flat symbol's container, a
/// nested one's detail head (`"kind — title"`, or the method signature's
/// `method`).
fn symbol_kind(symbol: &Value) -> String {
    if let Some(container) = symbol["containerName"].as_str() {
        return container.to_string();
    }
    symbol["detail"]
        .as_str()
        .map(|d| d.split(' ').next().unwrap_or(d).to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// An MCP `outline` result in [`lsp_outline`]'s form. A method child is
/// named by its member name (`store.save` → `save`).
fn mcp_outline(result: &Value) -> Vec<String> {
    fn walk(entries: &[Value], depth: usize, out: &mut Vec<String>) {
        for e in entries {
            let range = mcp_span_key(&e["range"]);
            let file = e["range"]["file"].as_str().unwrap();
            let id = e["entity_id"].as_str().unwrap();
            let name = if depth > 0 {
                id.rsplit('.').next().unwrap()
            } else {
                id
            };
            let mut line = format!(
                "{}{name} {} {}",
                "  ".repeat(depth),
                e["kind"].as_str().unwrap(),
                range.strip_prefix(&format!("{file} ")).unwrap()
            );
            if e["name_range"].is_object() {
                let selection = mcp_span_key(&e["name_range"]);
                line.push_str(&format!(
                    " @{}",
                    selection.strip_prefix(&format!("{file} ")).unwrap()
                ));
            }
            out.push(line);
            if let Some(children) = e["children"].as_array() {
                walk(children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        result
            .as_array()
            .unwrap_or_else(|| panic!("outline returned no list: {result}")),
        0,
        &mut out,
    );
    out
}

/// The codes of the diagnostics MCP `inspect` lists, sorted.
fn inspect_codes(result: &Value) -> Vec<String> {
    let mut codes: Vec<String> = result["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("inspect lists no diagnostics: {result}"))
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect();
    codes.sort();
    codes
}

// ── Cases and expected divergences ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    Lsp,
    Mcp,
}

/// One navigation question and the answer both surfaces should give.
struct Case {
    id: &'static str,
    fixture: &'static str,
    /// What a surface answers when it has no row in
    /// [`EXPECTED_DIVERGENCES`] (the plan's target design).
    target: &'static [(Surface, &'static [&'static str])],
}

/// One known disagreement between a surface and its case's target.
struct Divergence {
    /// The plan's row (05, "Current behaviour to pin"), named for the
    /// ticket that closes it.
    id: &'static str,
    case: &'static str,
    surface: Surface,
    /// What the surface answers today.
    today: &'static [&'static str],
}

/// The fixes for `logout`'s reference to `sesion_limit`, which exists
/// nowhere: replace it with the close match, or create it.
const FIXES_FOR_THE_UNRESOLVED_REFERENCE: &[&str] = &[
    "Create invariant stub for sesion_limit refactor login.spec 8:1-8:1 \\ninvariant sesion_limit \\\"sesion_limit\\\" {\\n  // TODO: fill in fields\\n}\\n",
    "Replace with 'session_limit' quickfix login.spec 6:15-6:27 session_limit",
];

const CASES: &[Case] = &[
    Case {
        id: "references_login_without_declaration",
        fixture: "nav",
        target: &[(Surface::Lsp, &[]), (Surface::Mcp, &[])],
    },
    Case {
        id: "references_session_limit_without_declaration",
        fixture: "nav",
        target: &[
            (Surface::Lsp, &["login.spec 2:15-2:28"]),
            (Surface::Mcp, &["login.spec 2:15-2:28 invariants"]),
        ],
    },
    Case {
        id: "references_session_limit_with_declaration",
        fixture: "nav",
        target: &[
            (
                Surface::Lsp,
                &["limit.spec 1:11-1:24", "login.spec 2:15-2:28"],
            ),
            (
                Surface::Mcp,
                &["limit.spec 1:11-1:24 -", "login.spec 2:15-2:28 invariants"],
            ),
        ],
    },
    Case {
        id: "search_references_with_other_filters",
        fixture: "nav",
        target: &[(Surface::Mcp, &[])],
    },
    Case {
        id: "prepare_rename_session_limit",
        fixture: "nav",
        target: &[(Surface::Lsp, &["limit.spec 1:11-1:24"])],
    },
    Case {
        id: "rename_session_limit",
        fixture: "rn",
        target: &[
            (
                Surface::Lsp,
                &[
                    "a.spec 1:11-1:24 session_cap",
                    "a.spec 5:15-5:28 session_cap",
                ],
            ),
            (
                Surface::Mcp,
                &[
                    "a.spec 1:11-1:24 session_cap",
                    "a.spec 5:15-5:28 session_cap",
                ],
            ),
        ],
    },
    Case {
        id: "lookup_sesion",
        fixture: "nav",
        target: &[
            (Surface::Lsp, &["session_limit"]),
            (Surface::Mcp, &["session_limit id 0.525"]),
        ],
    },
    Case {
        id: "inspect_alpha_in_a_cycle",
        fixture: "cyc",
        target: &[(Surface::Mcp, &["E006", "W006", "W020", "W061"])],
    },
    Case {
        id: "fixes_for_the_unresolved_reference",
        fixture: "nav",
        target: &[
            (Surface::Lsp, FIXES_FOR_THE_UNRESOLVED_REFERENCE),
            (Surface::Mcp, FIXES_FOR_THE_UNRESOLVED_REFERENCE),
        ],
    },
    Case {
        id: "outline_of_a_file",
        fixture: "nav",
        target: &[
            (
                Surface::Lsp,
                &[
                    "login behavior 1:1-3:2 @1:10-1:15",
                    "logout behavior 5:1-7:2 @5:10-5:16",
                ],
            ),
            (
                Surface::Mcp,
                &[
                    "login behavior 1:1-3:2 @1:10-1:15",
                    "logout behavior 5:1-7:2 @5:10-5:16",
                ],
            ),
        ],
    },
    Case {
        id: "outline_of_a_file_with_methods",
        fixture: "port",
        target: &[
            (
                Surface::Lsp,
                &[
                    "Item type 1:1-3:2 @1:6-1:10",
                    "store port 5:1-8:2 @5:6-5:11",
                    "  save method 7:3-7:34 @7:10-7:14",
                ],
            ),
            (
                Surface::Mcp,
                &[
                    "Item type 1:1-3:2 @1:6-1:10",
                    "store port 5:1-8:2 @5:6-5:11",
                    "  save method 7:3-7:34 @7:10-7:14",
                ],
            ),
        ],
    },
];

/// What each surface answers otherwise today. Later tickets delete rows;
/// none may be added to excuse a regression.
const EXPECTED_DIVERGENCES: &[Divergence] = &[];

/// What `surface` should answer to `case` now: its row's `today`, else the
/// case's target.
fn expected(case: &Case, surface: Surface) -> Vec<String> {
    let row = EXPECTED_DIVERGENCES
        .iter()
        .find(|d| d.case == case.id && d.surface == surface);
    let answer = match row {
        Some(row) => row.today,
        None => {
            case.target
                .iter()
                .find(|(s, _)| *s == surface)
                .unwrap_or_else(|| panic!("{} has no {surface:?} target", case.id))
                .1
        }
    };
    answer.iter().map(|s| s.to_string()).collect()
}

fn case(id: &str) -> &'static Case {
    CASES
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no case {id}"))
}

/// Assert each surface's answer to case `id` is what [`expected`] says.
fn assert_case(id: &str, answers: &[(Surface, Vec<String>)]) {
    let case = case(id);
    let mut failures = Vec::new();
    for (surface, got) in answers {
        let want = expected(case, *surface);
        if *got != want {
            let rows: Vec<&str> = EXPECTED_DIVERGENCES
                .iter()
                .filter(|d| d.case == id && d.surface == *surface)
                .map(|d| d.id)
                .collect();
            failures.push(format!(
                "{surface:?} answers {got:?}\n  but {} says {want:?}\n  \
                 (a closed divergence: delete its row; a new one: fix the surface)",
                if rows.is_empty() {
                    "the case's target".to_string()
                } else {
                    format!("EXPECTED_DIVERGENCES {rows:?}")
                }
            ));
        }
    }
    assert!(failures.is_empty(), "case `{id}`:\n{}", failures.join("\n"));
}

// ── Cases ───────────────────────────────────────────────────────────────

#[test]
fn references_to_an_entity_nothing_references() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/references",
        references_params(&p, "login.spec", 0, 10, false),
    );
    let mcp = mcp_tool(
        &p,
        "specforge.find_references",
        json!({"entity_id": "login"}),
    );
    assert_case(
        "references_login_without_declaration",
        &[
            (Surface::Lsp, lsp_locations(&p, &lsp)),
            (Surface::Mcp, mcp_references(&mcp)),
        ],
    );
}

#[test]
fn references_without_the_declaration() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/references",
        references_params(&p, "limit.spec", 0, 12, false),
    );
    let mcp = mcp_tool(
        &p,
        "specforge.find_references",
        json!({"entity_id": "session_limit"}),
    );
    assert_case(
        "references_session_limit_without_declaration",
        &[
            (Surface::Lsp, lsp_locations(&p, &lsp)),
            (Surface::Mcp, mcp_references(&mcp)),
        ],
    );
}

#[test]
fn references_with_the_declaration() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/references",
        references_params(&p, "limit.spec", 0, 12, true),
    );
    let mcp = mcp_tool(
        &p,
        "specforge.find_references",
        json!({"entity_id": "session_limit", "include_declaration": true}),
    );
    assert_case(
        "references_session_limit_with_declaration",
        &[
            (Surface::Lsp, lsp_locations(&p, &lsp)),
            (Surface::Mcp, mcp_references(&mcp)),
        ],
    );
}

#[test]
fn search_references_combines_with_the_other_filters() {
    let p = project("nav");
    let mcp = mcp_tool(
        &p,
        "specforge.search",
        json!({"query": "zzz", "kinds": ["invariant"], "references": "session_limit"}),
    );
    assert_case(
        "search_references_with_other_filters",
        &[(Surface::Mcp, search_ids(&mcp))],
    );
}

#[test]
fn the_renameable_range_is_the_token() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/prepareRename",
        position(&p, "limit.spec", 0, 12),
    );
    let key = if lsp.is_null() {
        Vec::new()
    } else {
        vec![range_key("limit.spec", &lsp)]
    };
    assert_case("prepare_rename_session_limit", &[(Surface::Lsp, key)]);
}

#[test]
fn rename_edits() {
    let p = project("rn");
    let mut params = position(&p, "a.spec", 0, 12);
    params["newName"] = json!("session_cap");
    let lsp = lsp_request(&p, "textDocument/rename", params);
    let mcp = mcp_tool(
        &p,
        "specforge.rename",
        json!({"entity_id": "session_limit", "new_name": "session_cap", "dry_run": true}),
    );
    assert_case(
        "rename_session_limit",
        &[
            (Surface::Lsp, workspace_edit_keys(&p, &lsp)),
            (Surface::Mcp, mcp_rename_keys(&mcp)),
        ],
    );
}

#[test]
fn lookup_of_a_misspelled_id() {
    let p = project("nav");
    let lsp = lsp_request(&p, "workspace/symbol", json!({"query": "sesion"}));
    let lsp_names: Vec<String> = lsp
        .as_array()
        .map(|symbols| {
            symbols
                .iter()
                .map(|s| s["name"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    let mcp = mcp_tool(&p, "specforge.search", json!({"query": "sesion"}));
    assert_case(
        "lookup_sesion",
        &[(Surface::Lsp, lsp_names), (Surface::Mcp, search_keys(&mcp))],
    );
}

#[test]
fn inspect_lists_a_cycle_among_the_entitys_diagnostics() {
    let p = project("cyc");
    let mcp = mcp_tool(&p, "specforge.inspect", json!({"entity_id": "alpha"}));
    assert_case(
        "inspect_alpha_in_a_cycle",
        &[(Surface::Mcp, inspect_codes(&mcp))],
    );
}

#[test]
fn fixes_for_an_unresolved_reference() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": p.uri("login.spec")},
            "range": {"start": {"line": 5, "character": 0}, "end": {"line": 5, "character": 30}},
            "context": {"diagnostics": []},
        }),
    );
    let mcp = mcp_tool(
        &p,
        "specforge.suggest_fixes",
        json!({"file_path": "login.spec", "diagnostic_code": "E003"}),
    );
    assert_case(
        "fixes_for_the_unresolved_reference",
        &[
            (Surface::Lsp, lsp_actions(&p, &lsp)),
            (Surface::Mcp, mcp_fixes(&mcp)),
        ],
    );
}

/// A client that declares hierarchical document symbols.
fn hierarchical() -> Value {
    json!({"textDocument": {"documentSymbol": {"hierarchicalDocumentSymbolSupport": true}}})
}

#[test]
fn outline_of_a_file() {
    let p = project("nav");
    let lsp = lsp_requests(
        &p,
        hierarchical(),
        vec![(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": p.uri("login.spec")}}),
        )],
    )
    .pop()
    .unwrap();
    let mcp = mcp_tool(&p, "specforge.outline", json!({"file": "login.spec"}));
    assert_case(
        "outline_of_a_file",
        &[
            (Surface::Lsp, lsp_outline(&p, "login.spec", &lsp)),
            (Surface::Mcp, mcp_outline(&mcp)),
        ],
    );
}

#[test]
fn outline_of_a_file_with_methods() {
    let p = project("port");
    let lsp = lsp_requests(
        &p,
        hierarchical(),
        vec![(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": p.uri("store.spec")}}),
        )],
    )
    .pop()
    .unwrap();
    let mcp = mcp_tool(&p, "specforge.outline", json!({"file": "store.spec"}));
    assert_case(
        "outline_of_a_file_with_methods",
        &[
            (Surface::Lsp, lsp_outline(&p, "store.spec", &lsp)),
            (Surface::Mcp, mcp_outline(&mcp)),
        ],
    );
}

/// MCP `find_references` answers the occurrences the LSP's references
/// answer, for each entity of each fixture, with and without the
/// declaration: the same locations, one per occurrence.
#[specforge_test(
    behavior = "provide_mcp_find_references_tool",
    verify = "find_references and the LSP's references answer the same occurrences"
)]
fn find_references_answers_what_the_lsp_answers() {
    // (fixture, file, 0-based line and character of an entity's name)
    let names: &[(&str, &str, u32, u32, &str)] = &[
        ("nav", "limit.spec", 0, 12, "session_limit"),
        ("nav", "login.spec", 0, 10, "login"),
        ("nav", "login.spec", 4, 10, "logout"),
        ("rn", "a.spec", 0, 12, "session_limit"),
        ("rn", "a.spec", 3, 10, "login"),
        ("port", "store.spec", 0, 6, "Item"),
        ("port", "store.spec", 4, 6, "store"),
    ];
    for &(fixture, file, line, character, id) in names {
        let p = project(fixture);
        for include_declaration in [false, true] {
            let lsp = lsp_request(
                &p,
                "textDocument/references",
                references_params(&p, file, line, character, include_declaration),
            );
            let mcp = mcp_tool(
                &p,
                "specforge.find_references",
                json!({"entity_id": id, "include_declaration": include_declaration}),
            );
            let mut mcp_locations: Vec<String> = mcp["locations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| mcp_span_key(&l["source_span"]))
                .collect();
            mcp_locations.sort();
            assert_eq!(
                lsp_locations(&p, &lsp),
                mcp_locations,
                "{fixture} {id}, declaration included: {include_declaration}"
            );
        }
    }
}

/// MCP search ranks as the LSP's workspace symbols and completion rank:
/// for each query, search's ids (over names; the fixture's string fields
/// hold none of these texts) are the workspace symbols in the same order,
/// and in a reference list completion offers what search finds of the
/// field's target kind.
#[specforge_test(
    behavior = "provide_mcp_search_tool",
    verify = "search ranks exactly as LSP workspaceSymbol and completion rank"
)]
fn search_ranks_as_workspace_symbols_and_completion() {
    let p = project("nav");
    let queries = [
        "", "session", "log", "sesion", "Login", "LIMIT", "lgout", "zzz",
    ];
    let mut requests: Vec<Request> = queries
        .iter()
        .map(|q| ("workspace/symbol", json!({"query": q})))
        .collect();
    requests.push(("textDocument/completion", position(&p, "login.spec", 1, 14)));
    let mut answers = lsp_requests(&p, json!({}), requests);
    let completion = answers.pop().unwrap();
    for (query, symbols) in queries.iter().zip(answers) {
        let lsp: Vec<String> = symbols
            .as_array()
            .map(|s| {
                s.iter()
                    .map(|s| s["name"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let mcp = mcp_tool(&p, "specforge.search", json!({"query": query}));
        assert_eq!(search_ids(&mcp), lsp, "query {query:?}");
    }
    let labels: Vec<String> = completion
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect();
    let mcp = mcp_tool(
        &p,
        "specforge.search",
        json!({"query": "session_limit", "kinds": ["invariant"]}),
    );
    assert_eq!(search_ids(&mcp), labels);
}

/// MCP suggest_fixes returns the fixes the LSP offers as code actions for
/// the same file, each with the same title, kind and edits.
#[specforge_test(
    behavior = "provide_mcp_suggest_fixes_tool",
    verify = "every suggestion carries the edits the LSP's code action applies"
)]
fn suggest_fixes_carries_the_code_actions_edits() {
    let nav = project("nav");
    let testing = project_of(
        &["@specforge/software", "@specforge/testing"],
        &[
            (
                "limit.spec",
                "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n",
            ),
            (
                "flows.spec",
                "behavior login \"Login\" {\n  contract \"c\"\n  invariants [sesion_limit]\n}\n\nbehavior audit \"Audit\" { contract \"c\" }\n",
            ),
        ],
    );
    for (p, file, lines) in [(&nav, "login.spec", 8), (&testing, "flows.spec", 7)] {
        let lsp = lsp_request(
            p,
            "textDocument/codeAction",
            json!({
                "textDocument": {"uri": p.uri(file)},
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": lines, "character": 0}},
                "context": {"diagnostics": []},
            }),
        );
        let mcp = mcp_tool(p, "specforge.suggest_fixes", json!({"file_path": file}));
        let actions = lsp_actions(p, &lsp);
        assert!(!actions.is_empty(), "{file}: no fixes to compare");
        assert_eq!(mcp_fixes(&mcp), actions, "{file}");
    }
}

/// The LSP's document symbols are the tree MCP outline returns: entities
/// in line order, methods nested as children, each selecting its name.
#[specforge_test(
    behavior = "outline_view",
    verify = "the outline nests an entity's methods as the MCP outline does"
)]
fn document_symbols_nest_as_the_mcp_outline() {
    for (fixture, file) in [
        ("port", "store.spec"),
        ("nav", "login.spec"),
        ("nav", "limit.spec"),
    ] {
        let p = project(fixture);
        let lsp = lsp_requests(
            &p,
            hierarchical(),
            vec![(
                "textDocument/documentSymbol",
                json!({"textDocument": {"uri": p.uri(file)}}),
            )],
        )
        .pop()
        .unwrap();
        let mcp = mcp_tool(&p, "specforge.outline", json!({"file": file}));
        assert_eq!(
            lsp_outline(&p, file, &lsp),
            mcp_outline(&mcp),
            "{fixture} {file}"
        );
    }
}

/// Every row names a case the harness runs, a surface the case answers
/// on, and differs from the case's target; every case names a fixture.
#[test]
fn navigation_table_is_consistent() {
    for row in EXPECTED_DIVERGENCES {
        let case = case(row.case);
        let target = case
            .target
            .iter()
            .find(|(s, _)| *s == row.surface)
            .unwrap_or_else(|| {
                panic!(
                    "{} names no {:?} target of {}",
                    row.id, row.surface, row.case
                )
            });
        assert_ne!(
            target.1, row.today,
            "{} on {} is its target: delete it",
            row.id, row.case
        );
    }
    for case in CASES {
        assert!(
            fixtures_dir()
                .join(case.fixture)
                .join("specforge.json")
                .is_file(),
            "{}: no fixture {}",
            case.id,
            case.fixture
        );
    }
}

// ── Pinned behaviour the plan keeps ─────────────────────────────────────

/// Write `text` to `file` in the project.
fn write(project: &Project, file: &str, text: &str) {
    fs::write(project.root.join(file), text).unwrap();
}

/// A project of `files` with `extensions`, in a temporary directory.
fn project_of(extensions: &[&str], files: &[(&str, &str)]) -> Project {
    let dir = TestProject::new()
        .raw_config(json!({"name": "pin", "extensions": extensions}).to_string())
        .files(files);
    let root = fs::canonicalize(dir.root()).unwrap();
    Project { _dir: dir, root }
}

#[test]
fn lsp_definition_crosses_files() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/definition",
        position(&p, "login.spec", 1, 16),
    );
    // The name, the client declaring no linkSupport.
    assert_eq!(location_key(&p, &lsp), "limit.spec 1:11-1:24", "{lsp}");
}

#[test]
fn lsp_definition_of_a_use_line_is_the_imported_file() {
    let p = project("nav");
    write(&p, "uses.spec", "use \"limit\"\n");
    let lsp = lsp_request(
        &p,
        "textDocument/definition",
        position(&p, "uses.spec", 0, 6),
    );
    // The imported file's first line.
    assert_eq!(location_key(&p, &lsp), "limit.spec 1:1-1:1", "{lsp}");
}

#[test]
fn lsp_completion_in_a_reference_list_offers_its_target_kind() {
    let p = project("nav");
    let lsp = lsp_request(
        &p,
        "textDocument/completion",
        position(&p, "login.spec", 1, 14),
    );
    let labels: Vec<&str> = lsp
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["session_limit"], "{lsp}");
}

#[test]
fn lsp_offers_a_verify_stub_for_a_testable_entity_without_obligations() {
    let p = project_of(
        &["@specforge/software", "@specforge/testing"],
        &[(
            "test.spec",
            "behavior foo \"Foo\" {\n  contract \"test\"\n}\n",
        )],
    );
    let lsp = lsp_request(
        &p,
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": p.uri("test.spec")},
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 2, "character": 1}},
            "context": {"diagnostics": []},
        }),
    );
    assert_eq!(
        lsp_actions(&p, &lsp),
        ["Add verify stub for foo quickfix test.spec 3:1-3:1   verify unit \\\"foo — TODO\\\"\\n"],
        "{lsp}"
    );
}

#[test]
fn lsp_offers_a_stub_for_a_reference_to_nothing() {
    let p = project_of(
        &["@specforge/software", "@specforge/testing"],
        &[(
            "auth.spec",
            "behavior login \"L\" {\n  invariants [session_limit]\n  verify unit \"y\"\n}\n",
        )],
    );
    let lsp = lsp_request(
        &p,
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": p.uri("auth.spec")},
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 4, "character": 0}},
            "context": {"diagnostics": []},
        }),
    );
    assert_eq!(
        lsp_actions(&p, &lsp),
        [
            "Create invariant stub for session_limit refactor auth.spec 5:1-5:1 \\ninvariant session_limit \\\"session_limit\\\" {\\n  // TODO: fill in fields\\n}\\n"
        ],
        "{lsp}"
    );
}

#[test]
fn lsp_ranges_are_utf16_after_multibyte_text() {
    let p = project_of(
        &["@specforge/software"],
        &[(
            "a.spec",
            "invariant cap \"é→\" { guarantee \"x\" }\nbehavior b \"é→\" { invariants [cap] }\n",
        )],
    );
    let lsp = lsp_request(
        &p,
        "textDocument/references",
        references_params(&p, "a.spec", 0, 11, true),
    );
    // The tokens: `b`'s follows "é→" (5 bytes, 2 UTF-16 units).
    assert_eq!(
        lsp_locations(&p, &lsp),
        ["a.spec 1:11-1:14", "a.spec 2:31-2:34"],
        "{lsp}"
    );
}

#[test]
fn mcp_find_definition_answers_file_line_and_column() {
    let p = project("nav");
    let mcp = mcp_tool(
        &p,
        "specforge.find_definition",
        json!({"entity_id": "session_limit"}),
    );
    // The name's position; the block and the name as spans.
    assert_eq!(
        mcp,
        json!({
            "entity_id": "session_limit", "file_path": "limit.spec", "line": 1, "column": 11,
            "source_span": {"file": "limit.spec", "start_line": 1, "start_col": 1, "end_line": 3, "end_col": 2},
            "name_span": {"file": "limit.spec", "start_line": 1, "start_col": 11, "end_line": 1, "end_col": 24},
            "precision": "token",
        })
    );
}

#[test]
fn mcp_an_unknown_entity_is_entity_not_found() {
    let p = project("nav");
    for tool in [
        "specforge.find_definition",
        "specforge.find_references",
        "specforge.inspect",
        "specforge.suggest_fixes",
    ] {
        let (result, failed) = mcp_result(&p, tool, json!({"entity_id": "nope"}));
        assert!(failed, "{tool}: {result}");
        let text = result["content"][0]["text"].as_str().unwrap();
        let error: Value = serde_json::from_str(text).unwrap();
        assert_eq!(error["code"], "entity_not_found", "{tool}: {result}");
    }
}

#[test]
fn mcp_outline_of_a_missing_file_is_file_not_found() {
    let p = project("nav");
    let (result, failed) = mcp_result(&p, "specforge.outline", json!({"file": "nope.spec"}));
    assert!(failed, "{result}");
    let error: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["code"], "file_not_found", "{result}");
}
