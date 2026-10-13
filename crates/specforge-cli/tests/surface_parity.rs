//! Parity harness (architecture plan 01, step P0).
//!
//! `specforge check`, MCP `specforge.validate`, the LSP and `specforge watch`
//! each assemble the compiler on their own. This harness runs every fixture
//! under `tests/fixtures/parity/` through all four and compares what each
//! reports with what `check` reports.
//!
//! Diagnostics are compared as multisets of `"CODE severity file:line"`
//! keys (`-` for a diagnostic without a span; the file is relative to the
//! spec root). The LSP cannot say everything `check` says: it must attach
//! a span-less diagnostic to some document, and puts one about entities
//! (its data names them) at the first one's name and any other on the
//! edited one at line 1, so `check`'s keys are projected onto that. Watch's events
//! carry the full list; its `rebuilt` event also gets a key when the debug
//! build's check of the incremental graph against a cold build fails.
//!
//! Where a surface disagrees with `check` today, the disagreement is written
//! down in [`EXPECTED_DIVERGENCES`]. The harness asserts that each surface
//! differs from `check` by exactly those rows: no more, no less. A later
//! step that closes a divergence makes this test fail until its row is
//! deleted, so a step can only turn green on purpose.
//!
//! The per-fixture tests characterize the surfaces and are not linked.
//! Three tests prove the obligations the plan names for this seam, over
//! every fixture: `check` prints the same diagnostics in the same order on
//! every run (`diagnostic_determinism`), watch's rebuilt graph is a cold
//! build's, with the diagnostics `check` reports (`incremental_correctness`),
//! and the LSP publishes what `check` reports (`shared_incremental_pipeline`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use specforge_test::prelude::*;
use tempfile::TempDir;

// ── Fixtures ────────────────────────────────────────────────────────────

/// What the LSP session does after opening the entry file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    Nothing,
    /// Rewrite `specforge.json` (a trailing newline) and report it changed (an
    /// extension-environment reload). The file must really change: an event
    /// for an unchanged file is no change to the session (ADR 0073).
    ReloadConfig,
    /// Delete this file (relative to the project root) and report it.
    Delete(&'static str),
}

/// Each fixture, the file the harness opens in the LSP and rewrites to make
/// watch rebuild (relative to the project root), and what the LSP session
/// does next.
const FIXTURES: &[(&str, &str, Then)] = &[
    ("clean", "main.spec", Then::Nothing),
    ("missing_import", "main.spec", Then::Nothing),
    ("unknown_extension", "main.spec", Then::Nothing),
    ("import_cycle", "a.spec", Then::Nothing),
    ("product_cycle", "main.spec", Then::Nothing),
    ("spec_root_set", "spec/main.spec", Then::ReloadConfig),
    ("exclude", "main.spec", Then::Nothing),
    ("body_parser_type", "main.spec", Then::Nothing),
    ("delete_file", "main.spec", Then::Delete("other.spec")),
];

// ── Expected divergences ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    /// MCP `specforge.validate`, through `McpServer::handle_message`.
    Mcp,
    /// The LSP `Backend`: every `publishDiagnostics` after opening the
    /// fixture's entry file.
    Lsp,
    /// What the LSP publishes after `specforge.json` changes
    /// ([`Then::ReloadConfig`]), against `check` on the same project.
    LspAfterReload,
    /// What the LSP publishes after a file is deleted ([`Then::Delete`]),
    /// against `check` on the project without that file.
    LspAfterDelete,
    /// The error and warning counts of watch's `ready` event.
    WatchReady,
    /// The error and warning counts of watch's `rebuilt` event after the
    /// entry file is rewritten.
    WatchRebuilt,
}

/// One known disagreement between a surface and `check`, on one fixture.
struct Divergence {
    /// The plan's bug ID (plan 01 §6), for the step that will close it.
    id: &'static str,
    fixture: &'static str,
    surface: Surface,
    /// Keys `check` reports that the surface does not.
    missing: &'static [&'static str],
    /// Keys the surface reports that `check` does not.
    extra: &'static [&'static str],
}

/// What each surface gets wrong today. Later steps delete rows; none may be
/// added to excuse a regression. The LSP's rows (D1, D3, D4, D5) closed
/// when it moved onto the shared project session (P6).
const EXPECTED_DIVERGENCES: &[Divergence] = &[];

// ── Normalized diagnostics ──────────────────────────────────────────────

/// `"CODE severity file:line"`, or `"CODE severity -"` without a span.
fn key(code: &str, severity: &str, location: Option<(String, u64)>) -> String {
    let severity = severity.to_ascii_lowercase();
    match location {
        Some((file, line)) => format!("{code} {severity} {file}:{line}"),
        None => format!("{code} {severity} -"),
    }
}

/// Counts of each key: a multiset.
type Keys = BTreeMap<String, usize>;

fn multiset<I: IntoIterator<Item = String>>(keys: I) -> Keys {
    let mut set = Keys::new();
    for k in keys {
        *set.entry(k).or_default() += 1;
    }
    set
}

/// The keys in `a` beyond those in `b`, sorted, with repeats.
fn difference(a: &Keys, b: &Keys) -> Vec<String> {
    let mut out = Vec::new();
    for (k, &n) in a {
        let m = b.get(k).copied().unwrap_or(0);
        for _ in m..n {
            out.push(k.clone());
        }
    }
    out
}

/// What `check` reports, projected onto what the LSP publishes. A
/// diagnostic without a span has to be attached to some document: one
/// about entities (its data names them) goes at the first one's name (its
/// file and line, as MCP find_definition answers them); one about none
/// on the document being edited, at its first line.
fn as_published(project: &Project, edited: &str) -> Keys {
    let out = check_output(project);
    let diagnostics: Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("check JSON ({e}): {out}"));
    let mut keys = Vec::new();
    for d in diagnostics.as_array().unwrap() {
        let code = d["code"].as_str().unwrap();
        let severity = d["severity"].as_str().unwrap();
        let location = match d["span"].as_object() {
            Some(s) => (
                s["file"].as_str().unwrap().to_string(),
                s["start_line"].as_u64().unwrap(),
            ),
            None => subject_name(project, &d["data"]).unwrap_or((edited.to_string(), 1)),
        };
        keys.push(key(code, severity, Some(location)));
    }
    multiset(keys)
}

/// The entities a diagnostic's data names, in order: an unresolved
/// reference's holder, a cycle's path, a pass diagnostic's subject.
fn data_entities(data: &Value) -> Vec<String> {
    let names: Vec<&Value> = match data["kind"].as_str() {
        Some("unresolved_reference" | "subject") => vec![&data["entity"]],
        Some("reference_cycle") => data["path"].as_array().unwrap().iter().collect(),
        _ => Vec::new(),
    };
    names
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// The file and line of the name of the first entity `data` names that
/// the project declares (MCP find_definition's answer).
fn subject_name(project: &Project, data: &Value) -> Option<(String, u64)> {
    data_entities(data).into_iter().find_map(|entity| {
        let mut server = specforge_mcp::McpServer::new();
        let mut call = |method: &str, params: Value| -> Value {
            let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
            serde_json::from_str(&server.handle_message(&req.to_string()).unwrap()).unwrap()
        };
        call(
            "initialize",
            json!({"projectRoot": project.root.to_str().unwrap()}),
        );
        let resp = call(
            "tools/call",
            json!({"name": "specforge.find_definition", "arguments": {"entity_id": entity}}),
        );
        if resp["result"]["isError"] == true {
            return None;
        }
        let found: Value =
            serde_json::from_str(resp["result"]["content"][0]["text"].as_str()?).ok()?;
        Some((
            found["file_path"].as_str()?.to_string(),
            found["line"].as_u64()?,
        ))
    })
}

// ── The project under test ──────────────────────────────────────────────

struct Project {
    _dir: TempDir,
    /// Canonical project root.
    root: PathBuf,
    /// Canonical spec root (the root, or `root/spec_root`).
    spec_root: PathBuf,
    /// The fixture's entry file, absolute.
    entry: PathBuf,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn project(fixture: &str, entry: &str) -> Project {
    let dir = TempDir::new().unwrap();
    copy_tree(&fixtures_dir().join(fixture), dir.path());
    let root = fs::canonicalize(dir.path()).unwrap();
    let config: Value =
        serde_json::from_str(&fs::read_to_string(root.join("specforge.json")).unwrap()).unwrap();
    let spec_root = match config["spec_root"].as_str() {
        Some(sr) => root.join(sr),
        None => root.clone(),
    };
    Project {
        entry: root.join(entry),
        _dir: dir,
        root,
        spec_root,
    }
}

impl Project {
    /// `path` relative to the spec root, as `check` prints it.
    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.spec_root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
}

// ── check ───────────────────────────────────────────────────────────────

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_specforge")
}

/// `specforge check --format json`, as printed.
fn check_output(project: &Project) -> String {
    check_output_with(project, &[])
}

/// `specforge check --format json` with `flags`, as printed.
fn check_output_with(project: &Project, flags: &[&str]) -> String {
    let out = Command::new(binary())
        .args(["check", "--format", "json"])
        .args(flags)
        .arg("--path")
        .arg(&project.root)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

/// `specforge check --format json`, as keys in output order.
fn check(project: &Project) -> Vec<String> {
    let out = check_output(project);
    let diagnostics: Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("check JSON ({e}): {out}"));
    keys_of(&diagnostics)
}

/// The keys of a diagnostic list serialized as `check` prints it (nested
/// `span`), in list order.
fn keys_of(diagnostics: &Value) -> Vec<String> {
    diagnostics
        .as_array()
        .unwrap_or_else(|| panic!("not a diagnostic list: {diagnostics}"))
        .iter()
        .map(|d| {
            let location = d["span"].as_object().map(|s| {
                (
                    s["file"].as_str().unwrap().to_string(),
                    s["start_line"].as_u64().unwrap(),
                )
            });
            key(
                d["code"].as_str().unwrap(),
                d["severity"].as_str().unwrap(),
                location,
            )
        })
        .collect()
}

// ── MCP ─────────────────────────────────────────────────────────────────

fn mcp(project: &Project) -> Keys {
    mcp_validate(project, json!({}))
}

/// MCP `specforge.validate` with `arguments`, as keys.
fn mcp_validate(project: &Project, arguments: Value) -> Keys {
    let diagnostics = mcp_validate_json(project, arguments);
    multiset(diagnostics.as_array().unwrap().iter().map(|d| {
        let location = d["file"]
            .as_str()
            .map(|f| (f.to_string(), d["line"].as_u64().unwrap()));
        key(
            d["code"].as_str().unwrap(),
            d["severity"].as_str().unwrap(),
            location,
        )
    }))
}

/// MCP `specforge.validate` with `arguments`: the diagnostics it returns.
fn mcp_validate_json(project: &Project, arguments: Value) -> Value {
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
    let resp = call(
        "tools/call",
        json!({"name": "specforge.validate", "arguments": arguments}),
    );
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("validate returned no text: {resp}"));
    serde_json::from_str(text).unwrap()
}

// ── LSP ─────────────────────────────────────────────────────────────────

pub(crate) mod lsp {
    use serde_json::{Value, json};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
    use tokio::time::{Duration, Instant, timeout};
    use tower_lsp::lsp_types::Url;
    use tower_lsp::{LspService, Server};

    /// A minimal in-process LSP client over memory streams.
    pub struct Client {
        writer: DuplexStream,
        reader: DuplexStream,
        next_id: i64,
        server: tokio::task::JoinHandle<()>,
    }

    impl Drop for Client {
        fn drop(&mut self) {
            self.server.abort();
        }
    }

    impl Client {
        pub fn start() -> Self {
            let (writer, server_in) = tokio::io::duplex(1 << 20);
            let (server_out, reader) = tokio::io::duplex(1 << 20);
            let (service, socket) = LspService::new(specforge_lsp::backend::Backend::new);
            let server = tokio::spawn(async move {
                Server::new(server_in, server_out, socket)
                    .serve(service)
                    .await;
            });
            Client {
                writer,
                reader,
                next_id: 1,
                server,
            }
        }

        async fn write(&mut self, msg: &Value) {
            let body = msg.to_string();
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            self.writer.write_all(header.as_bytes()).await.unwrap();
            self.writer.write_all(body.as_bytes()).await.unwrap();
            self.writer.flush().await.unwrap();
        }

        async fn read(&mut self) -> Value {
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0u8; 1];
                self.reader.read_exact(&mut byte).await.unwrap();
                header.push(byte[0]);
            }
            let length: usize = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let mut body = vec![0u8; length];
            self.reader.read_exact(&mut body).await.unwrap();
            let msg: Value = serde_json::from_slice(&body).unwrap();
            // Answer server-to-client requests (progress creation,
            // capability registration) so the server never waits.
            if msg.get("method").is_some() && msg.get("id").is_some() {
                self.write(&json!({"jsonrpc": "2.0", "id": msg["id"], "result": null}))
                    .await;
            }
            msg
        }

        pub(crate) async fn request(&mut self, method: &str, params: Value) -> Value {
            let id = self.next_id;
            self.next_id += 1;
            self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
                .await;
            loop {
                let msg = self.read().await;
                if msg.get("method").is_none() && msg["id"] == id {
                    return msg;
                }
            }
        }

        pub(crate) async fn notify(&mut self, method: &str, params: Value) {
            self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
                .await;
        }

        /// Initialize on `root` and wait until background indexing ends.
        pub async fn open_workspace(&mut self, root: &std::path::Path) {
            self.open_workspace_with(root, json!({})).await;
        }

        /// Initialize on `root` as a client declaring `capabilities`, and
        /// wait until background indexing ends.
        pub(crate) async fn open_workspace_with(
            &mut self,
            root: &std::path::Path,
            capabilities: Value,
        ) {
            let uri = Url::from_file_path(root).unwrap().to_string();
            self.request(
                "initialize",
                json!({"processId": null, "rootUri": uri, "capabilities": capabilities}),
            )
            .await;
            self.notify("initialized", json!({})).await;
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let msg = timeout(left, self.read())
                    .await
                    .expect("LSP indexing never ended");
                if msg["method"] == "$/progress" && msg["params"]["value"]["kind"] == "end" {
                    return;
                }
            }
        }

        /// Collect every `publishDiagnostics` until the server goes quiet:
        /// the last list published for each URI. Waits up to `first` for
        /// the first publish, then until `quiet` passes without one.
        async fn collect(&mut self, first: Duration, quiet: Duration) -> super::Published {
            let mut published = super::Published::new();
            let deadline = Instant::now() + first;
            loop {
                let wait = if published.is_empty() {
                    deadline.saturating_duration_since(Instant::now())
                } else {
                    quiet
                };
                let Ok(msg) = timeout(wait, self.read()).await else {
                    return published;
                };
                if msg["method"] == "textDocument/publishDiagnostics" {
                    let params = &msg["params"];
                    published.insert(
                        params["uri"].as_str().unwrap().to_string(),
                        params["diagnostics"].as_array().unwrap().clone(),
                    );
                }
            }
        }

        /// Open `path` and collect what the server publishes.
        pub async fn open(&mut self, path: &std::path::Path) -> super::Published {
            let uri = Url::from_file_path(path).unwrap().to_string();
            let text = std::fs::read_to_string(path).unwrap();
            self.notify(
                "textDocument/didOpen",
                json!({"textDocument": {
                    "uri": uri, "languageId": "specforge", "version": 1, "text": text
                }}),
            )
            .await;
            let published = self
                .collect(Duration::from_secs(60), Duration::from_secs(1))
                .await;
            assert!(
                published.contains_key(&uri),
                "the LSP never published diagnostics for {uri}"
            );
            published
        }

        /// Report `path` as changed (2) or deleted (3) and collect what the
        /// server publishes in response.
        pub async fn watched_file(
            &mut self,
            path: &std::path::Path,
            change: u8,
        ) -> super::Published {
            let uri = Url::from_file_path(path).unwrap().to_string();
            self.notify(
                "workspace/didChangeWatchedFiles",
                json!({"changes": [{"uri": uri, "type": change}]}),
            )
            .await;
            self.collect(Duration::from_secs(10), Duration::from_secs(1))
                .await
        }
    }
}

/// The last diagnostics published for each URI.
type Published = BTreeMap<String, Vec<Value>>;

/// What one LSP session published: after opening the entry file, and after
/// the fixture's [`Then`] step, if any.
struct LspRun {
    opened: Keys,
    then: Option<Keys>,
}

fn lsp(project: &Project, then: Then) -> LspRun {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let (opened, then) = runtime.block_on(async {
        let mut client = lsp::Client::start();
        client.open_workspace(&project.root).await;
        let opened = client.open(&project.entry).await;
        let then = match then {
            Then::Nothing => None,
            Then::ReloadConfig => Some({
                let config = project.root.join("specforge.json");
                let text = fs::read_to_string(&config).unwrap();
                fs::write(&config, format!("{text}\n")).unwrap();
                client
                    .watched_file(&project.root.join("specforge.json"), 2)
                    .await
            }),
            Then::Delete(file) => {
                let path = project.root.join(file);
                fs::remove_file(&path).unwrap();
                Some(client.watched_file(&path, 3).await)
            }
        };
        (opened, then)
    });
    LspRun {
        opened: published_keys(project, opened),
        then: then.map(|p| published_keys(project, p)),
    }
}

fn published_keys(project: &Project, published: Published) -> Keys {
    let mut keys = Vec::new();
    for (uri, diagnostics) in published {
        let path = tower_lsp::lsp_types::Url::parse(&uri)
            .unwrap()
            .to_file_path()
            .unwrap();
        let file = project.relative(&path);
        for d in diagnostics {
            let severity = match d["severity"].as_u64() {
                Some(1) => "error",
                Some(2) => "warning",
                Some(3) => "info",
                other => panic!("unexpected LSP severity {other:?}: {d}"),
            };
            let line = d["range"]["start"]["line"].as_u64().unwrap() + 1;
            keys.push(key(
                d["code"].as_str().unwrap(),
                severity,
                Some((file.clone(), line)),
            ));
        }
    }
    multiset(keys)
}

// ── watch ───────────────────────────────────────────────────────────────

/// Wait for a JSON event line whose `event` is `name`.
fn wait_for_event(rx: &mpsc::Receiver<String>, name: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if let Ok(line) = rx.recv_timeout(Duration::from_secs(1))
            && let Ok(event) = serde_json::from_str::<Value>(&line)
            && event["event"] == name
        {
            return event;
        }
    }
    panic!("watch never reported {name}");
}

/// The key a `rebuilt` event adds when the debug build's check of the
/// incremental graph against a cold rebuild fails.
const INCREMENTAL_MISMATCH: &str = "incremental graph differs from a cold build";

/// The keys of the diagnostics an event lists, plus
/// [`INCREMENTAL_MISMATCH`] when the event's verification failed.
fn event_keys(event: &Value) -> Keys {
    let mut keys = multiset(keys_of(&event["diagnostics"]));
    if event["verification_failed"] == true {
        keys.insert(INCREMENTAL_MISMATCH.to_string(), 1);
    }
    keys
}

/// Watch's `ready` diagnostics, then its `rebuilt` diagnostics after the
/// entry file is rewritten with the same entities.
fn watch(project: &Project) -> (Keys, Keys) {
    let mut watch = Command::new(binary());
    watch
        .args(["watch", "--format", "json", "--path"])
        .arg(&project.root);
    let mut child = crate::child_guard::ChildGuard::spawn(
        crate::child_guard::guarded_command(&watch)
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let stdout = child.take_stdout().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let ready = wait_for_event(&rx, "ready");
    std::thread::sleep(Duration::from_millis(300));
    let text = fs::read_to_string(&project.entry).unwrap();
    fs::write(&project.entry, format!("{text}\n")).unwrap();
    let rebuilt = wait_for_event(&rx, "rebuilt");
    drop(child);
    // A debug build checks every rebuild against a cold one; the harness
    // relies on that to see incremental drift.
    assert!(
        !rebuilt["verification"].is_null(),
        "watch did not verify the rebuild: {rebuilt}"
    );
    (event_keys(&ready), event_keys(&rebuilt))
}

// ── The comparison ──────────────────────────────────────────────────────

/// The difference the table expects for one surface on one fixture:
/// `(missing, extra)`, sorted.
fn expected(fixture: &str, surface: Surface) -> (Vec<String>, Vec<String>) {
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for row in EXPECTED_DIVERGENCES
        .iter()
        .filter(|d| d.fixture == fixture && d.surface == surface)
    {
        missing.extend(row.missing.iter().map(|s| s.to_string()));
        extra.extend(row.extra.iter().map(|s| s.to_string()));
    }
    missing.sort();
    extra.sort();
    (missing, extra)
}

fn compare(fixture: &str, surface: Surface, check: &Keys, got: &Keys, failures: &mut Vec<String>) {
    let found = (difference(check, got), difference(got, check));
    let want = expected(fixture, surface);
    if found == want {
        return;
    }
    let ids: Vec<&str> = EXPECTED_DIVERGENCES
        .iter()
        .filter(|d| d.fixture == fixture && d.surface == surface)
        .map(|d| d.id)
        .collect();
    failures.push(format!(
        "{surface:?} on `{fixture}` differs from check by\n    \
         missing {:?}, extra {:?}\n  but EXPECTED_DIVERGENCES {ids:?} says\n    \
         missing {:?}, extra {:?}\n  \
         (a closed divergence: delete its row; a new one: fix the surface)",
        found.0, found.1, want.0, want.1
    ));
}

fn assert_parity(fixture: &str) {
    let &(_, entry, then) = FIXTURES
        .iter()
        .find(|(name, _, _)| *name == fixture)
        .unwrap();
    let project = project(fixture, entry);

    // The baseline must hold still: the same keys in the same order on a
    // second run (messages are not compared, see the module docs).
    let first = check(&project);
    let second = check(&project);
    assert_eq!(first, second, "check is not deterministic on `{fixture}`");
    let check_keys = multiset(first);

    let mut failures = Vec::new();
    compare(
        fixture,
        Surface::Mcp,
        &check_keys,
        &mcp(&project),
        &mut failures,
    );

    let (ready, rebuilt) = watch(&project);
    compare(
        fixture,
        Surface::WatchReady,
        &check_keys,
        &ready,
        &mut failures,
    );
    compare(
        fixture,
        Surface::WatchRebuilt,
        &check_keys,
        &rebuilt,
        &mut failures,
    );

    // Last: its Then step may delete a file.
    compare_lsp(fixture, &project, then, &mut failures);

    assert!(
        failures.is_empty(),
        "check reports {check_keys:?} on `{fixture}`\n{}",
        failures.join("\n")
    );
}

/// What the LSP publishes after opening the entry file, and after the
/// fixture's [`Then`] step, against what `check` reports then.
fn compare_lsp(fixture: &str, project: &Project, then: Then, failures: &mut Vec<String>) {
    let edited = project.relative(&project.entry);
    let published = as_published(project, &edited);
    let run = lsp(project, then);
    compare(fixture, Surface::Lsp, &published, &run.opened, failures);
    match (then, run.then) {
        (Then::ReloadConfig, Some(after)) => compare(
            fixture,
            Surface::LspAfterReload,
            &published,
            &after,
            failures,
        ),
        (Then::Delete(_), Some(after)) => {
            let now = as_published(project, &edited);
            compare(fixture, Surface::LspAfterDelete, &now, &after, failures)
        }
        _ => {}
    }
}

// ── Tests: one per fixture ──────────────────────────────────────────────

#[test]
fn parity_clean() {
    assert_parity("clean");
}

#[test]
fn parity_missing_import() {
    assert_parity("missing_import");
}

#[test]
fn parity_unknown_extension() {
    assert_parity("unknown_extension");
}

#[test]
fn parity_import_cycle() {
    assert_parity("import_cycle");
}

#[test]
fn parity_product_cycle() {
    assert_parity("product_cycle");
}

#[test]
fn parity_spec_root_set() {
    assert_parity("spec_root_set");
}

#[test]
fn parity_exclude() {
    assert_parity("exclude");
}

#[test]
fn parity_body_parser_type() {
    assert_parity("body_parser_type");
}

#[test]
fn parity_delete_file() {
    assert_parity("delete_file");
}

/// `check` prints the same diagnostics, messages included, in the same
/// order on every run: no W113 cycle named in HashMap order, no file
/// order leaking into the output.
#[specforge_test(
    invariant = "diagnostic_determinism",
    verify = "identical source files produce identical diagnostics in the same order"
)]
fn check_prints_the_same_diagnostics_on_every_run() {
    for &(fixture, entry, _) in FIXTURES {
        let project = project(fixture, entry);
        let first = check_output(&project);
        for _ in 0..2 {
            assert_eq!(
                check_output(&project),
                first,
                "`{fixture}` changed between runs"
            );
        }
    }
}

/// On every fixture, watch's graph after a rebuild is a cold build's (the
/// debug build compares them) and its diagnostics are `check`'s.
#[specforge_test(
    invariant = "incremental_correctness",
    verify = "incremental recompilation produces the same graph as a full rebuild"
)]
fn watch_rebuilds_what_a_cold_build_builds_on_every_fixture() {
    let mut failures = Vec::new();
    for &(fixture, entry, _) in FIXTURES {
        let project = project(fixture, entry);
        let check_keys = multiset(check(&project));
        let (_, rebuilt) = watch(&project);
        compare(
            fixture,
            Surface::WatchRebuilt,
            &check_keys,
            &rebuilt,
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// On every fixture the LSP publishes what `check` reports: after opening
/// a file (E025 and W113 from the resolver included), after `specforge.json`
/// changes (the spec root is indexed again, and everything republished),
/// and after a file is deleted (the other files keep their extension-rule
/// diagnostics).
#[specforge_test(
    behavior = "shared_incremental_pipeline",
    verify = "the LSP publishes the diagnostics specforge check reports"
)]
fn lsp_publishes_what_check_reports_on_every_fixture() {
    let mut failures = Vec::new();
    for &(fixture, entry, then) in FIXTURES {
        let project = project(fixture, entry);
        compare_lsp(fixture, &project, then, &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// MCP validate applies the same diagnostic policy as `check`: the
/// `inferred` lint profile adds I200 (a source changed since it was
/// inferred) and I202 (dense inference), and strict promotes warnings, on
/// both surfaces alike.
#[specforge_test(
    behavior = "provide_mcp_validate_tool",
    verify = "validate with lint profiles reports what specforge check reports with the same profiles"
)]
fn mcp_validate_applies_the_lint_profiles_check_applies() {
    let project = project("product_cycle", "main.spec");
    fs::create_dir_all(project.root.join("src")).unwrap();
    fs::write(project.root.join("src/lib.rs"), "fn a() {}\n").unwrap();
    let manifest = json!({
        "version": 1,
        "source_roots": ["src"],
        "source_index": [{
            "path": "src/lib.rs",
            "content_hash": "not-the-hash-of-the-file",
            "entities_produced": ["a", "b"],
            "analyzed_at": "2026-01-01T00:00:00Z",
        }],
    });
    fs::write(
        project.root.join("specforge-infer.json"),
        manifest.to_string(),
    )
    .unwrap();

    for strict in [false, true] {
        let mut flags = vec!["--lint", "inferred"];
        if strict {
            flags.push("--strict");
        }
        let out = check_output_with(&project, &flags);
        let checked = multiset(keys_of(&serde_json::from_str(&out).unwrap()));
        for code in ["I200", "I202"] {
            assert!(
                checked.keys().any(|k| k.starts_with(code)),
                "check --lint inferred reports no {code}: {checked:?}"
            );
        }
        let validated = mcp_validate(&project, json!({"lint": ["inferred"], "strict": strict}));
        assert_eq!(validated, checked, "strict: {strict}");
    }
}

/// `check --severity` and MCP validate's `severity_filter` show the same
/// diagnostics on every fixture, for each severity, with and without
/// strict promotion (one filter, the check operation's), and the filter's
/// name matches ignoring case on both.
#[specforge_test(
    behavior = "filter_reported_diagnostics",
    verify = "check --severity and MCP validate severity_filter report the same diagnostics"
)]
fn check_severity_and_mcp_severity_filter_agree_on_every_fixture() {
    let mut failures = Vec::new();
    for &(fixture, entry, _) in FIXTURES {
        let project = project(fixture, entry);
        let everything = multiset(check(&project));
        let mut shown_total = 0;
        for severity in ["error", "warning", "Info"] {
            for strict in [false, true] {
                let mut flags = vec!["--severity", severity];
                let mut arguments = json!({"severity_filter": severity});
                if strict {
                    flags.push("--strict");
                    arguments["strict"] = json!(true);
                }
                let out = check_output_with(&project, &flags);
                let checked = multiset(keys_of(&serde_json::from_str(&out).unwrap()));
                let validated = mcp_validate(&project, arguments);
                if checked != validated {
                    failures.push(format!(
                        "{fixture} --severity {severity} strict={strict}: check {checked:?}, validate {validated:?}"
                    ));
                }
                if !strict {
                    shown_total += checked.values().sum::<usize>();
                }
            }
        }
        // The three filters partition what check reports.
        assert_eq!(shown_total, everything.values().sum::<usize>(), "{fixture}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `check --format json` and MCP validate print the same entries, key for
/// key: one presenter (the nested span and the flat location together).
#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "check and MCP validate present the same diagnostics identically"
)]
fn check_and_mcp_validate_present_the_same_json() {
    let entries = |diagnostics: Value| -> Vec<String> {
        let mut entries: Vec<String> = diagnostics
            .as_array()
            .unwrap()
            .iter()
            .map(Value::to_string)
            .collect();
        entries.sort();
        entries
    };
    for &(fixture, entry, _) in FIXTURES {
        let project = project(fixture, entry);
        let checked: Value = serde_json::from_str(&check_output(&project)).unwrap();
        let validated = mcp_validate_json(&project, json!({}));
        assert_eq!(
            entries(checked),
            entries(validated),
            "`{fixture}` is printed differently"
        );
    }
}

/// Every row names a fixture the harness runs, and every fixture exists.
#[test]
fn parity_table_names_real_fixtures() {
    for row in EXPECTED_DIVERGENCES {
        assert!(
            FIXTURES.iter().any(|(name, _, _)| *name == row.fixture),
            "{} names unknown fixture `{}`",
            row.id,
            row.fixture
        );
        assert!(
            !row.missing.is_empty() || !row.extra.is_empty(),
            "{} on `{}` is empty",
            row.id,
            row.fixture
        );
    }
    for (name, entry, then) in FIXTURES {
        let dir = fixtures_dir().join(name);
        assert!(
            dir.join("specforge.json").is_file(),
            "{name}: no specforge.json"
        );
        assert!(dir.join(entry).is_file(), "{name}: no {entry}");
        if let Then::Delete(file) = then {
            assert!(dir.join(file).is_file(), "{name}: no {file}");
        }
    }
}
