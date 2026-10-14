//! The one JSON-RPC session the LSP's tests drive a server through: an
//! in-process server over memory streams, its transport, a buffer of the
//! server's notifications, and the typed requests the tests send.

use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::task::JoinHandle;
use tower_lsp::{LspService, Server};

/// A client of an in-process LSP server, over memory streams.
pub struct Session {
    writer: DuplexStream,
    reader: DuplexStream,
    next_id: i64,
    server: JoinHandle<()>,
    /// Server notifications and server-to-client requests (already
    /// answered) not yet taken by `notification`.
    pending: Vec<Value>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// The `file://` URI of `path`.
pub fn uri_of(path: &Path) -> String {
    tower_lsp::lsp_types::Url::from_file_path(path)
        .unwrap()
        .to_string()
}

/// A project of the vendored docref extension (`fixtures/docref-extension`:
/// a `gadget` kind whose `docs` field names files, `@sdk/docref=ext/docref.wasm`
/// beside `@specforge/software`): spec root `spec/`, `spec/a.spec` holding
/// `spec`, and an empty `docs/` beside it.
pub fn docref_project(spec: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    for sub in ["spec", "ext", "docs"] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    let config = json!({
        "name": "p",
        "version": "0.1.0",
        "spec_root": "spec",
        "extensions": ["@specforge/software", "@sdk/docref=ext/docref.wasm"],
    });
    std::fs::write(root.join("specforge.json"), config.to_string()).unwrap();
    std::fs::copy(
        specforge_component::committed::DOCREF.path(),
        root.join("ext/docref.wasm"),
    )
    .unwrap();
    std::fs::write(root.join("spec/a.spec"), spec).unwrap();
    dir
}

/// A gadget of the docref project naming `../docs/guide.md` (from `spec/`:
/// `docs/guide.md` under the root), which does not exist.
pub const NAMES_GUIDE: &str = "gadget gadget_one \"G\" {\n  docs [\"../docs/guide.md\"]\n}\n";

/// The `didChangeWatchedFiles` watchers of a `client/registerCapability` request.
pub fn registered_globs(registration: &Value) -> Vec<String> {
    registration["params"]["registrations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["method"] == "workspace/didChangeWatchedFiles")
        .flat_map(|r| r["registerOptions"]["watchers"].as_array().unwrap().clone())
        .map(|w| w["globPattern"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The codes of `diagnostics`.
pub fn codes(diagnostics: &[Value]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|d| d["code"].as_str().unwrap_or(""))
        .collect()
}

impl Session {
    /// A server, started; nothing sent to it yet.
    pub fn spawn() -> Session {
        let (client_to_server, server_stdin) = tokio::io::duplex(1 << 20);
        let (server_stdout, server_to_client) = tokio::io::duplex(1 << 20);
        let (service, socket) = LspService::new(specforge_lsp::backend::Backend::new);
        let server = tokio::spawn(async move {
            Server::new(server_stdin, server_stdout, socket)
                .serve(service)
                .await;
        });
        Session {
            writer: client_to_server,
            reader: server_to_client,
            next_id: 1,
            server,
            pending: Vec::new(),
        }
    }

    /// A server sent `initialize` (`root_uri` as rootUri, `capabilities`
    /// declared) and `initialized`, without waiting for anything; with
    /// the `initialize` response.
    pub async fn launch(root_uri: Option<&str>, capabilities: Value) -> (Session, Value) {
        let mut session = Session::spawn();
        let init = session.initialize_with(root_uri, capabilities).await;
        session.initialized().await;
        (session, init)
    }

    /// Start a server, send `initialize` (with `root` as rootUri) and
    /// `initialized`, and wait until workspace indexing has ended.
    /// Returns the session and the `initialize` result.
    pub async fn start(root: Option<&Path>) -> (Session, Value) {
        Self::start_with_capabilities(root, json!({})).await
    }

    /// [`Self::start`] with the client declaring `capabilities` in its
    /// `initialize` request.
    pub async fn start_with_capabilities(
        root: Option<&Path>,
        capabilities: Value,
    ) -> (Session, Value) {
        let root_uri = root.map(|r| r.to_str().unwrap().to_string());
        let (mut session, init) = Self::launch(root_uri.as_deref(), capabilities).await;
        session
            .notification("$/progress", |p| p["value"]["kind"] == "end")
            .await
            .expect("workspace indexing never ended");
        (session, init["result"].clone())
    }

    /// A server with no project, `text` open as `file:///test/<file_name>`
    /// once its first diagnostics are published.
    pub async fn with_doc(
        root_uri: Option<&str>,
        file_name: &str,
        text: &str,
    ) -> (Session, String) {
        let (mut session, _) = Self::launch(root_uri, json!({})).await;
        let uri = format!("file:///test/{file_name}");
        session.did_open(&uri, "specforge", text).await;
        session
            .wait_for_notification("textDocument/publishDiagnostics", 5000)
            .await;
        (session, uri)
    }

    /// A server over a project whose `specforge.json` lists `extensions`,
    /// `text` written as `file_name` and open once the extensions are
    /// loaded; with the directory (keep it alive).
    pub async fn with_extensions(
        extensions: &[&str],
        file_name: &str,
        text: &str,
    ) -> (Session, String, tempfile::TempDir) {
        let (session, uri, dir, _) =
            Self::with_extensions_as(extensions, file_name, text, json!({})).await;
        (session, uri, dir)
    }

    /// [`Self::with_extensions`] for a client declaring `capabilities`,
    /// also returning the `initialize` response.
    pub async fn with_extensions_as(
        extensions: &[&str],
        file_name: &str,
        text: &str,
        capabilities: Value,
    ) -> (Session, String, tempfile::TempDir, Value) {
        let dir = tempfile::TempDir::new().unwrap();
        let config = json!({
            "name": "test",
            "version": "0.1.0",
            "extensions": extensions,
        });
        std::fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
        std::fs::write(dir.path().join(file_name), text).unwrap();

        let root = dir.path().to_str().unwrap();
        let (mut session, init) = Self::launch(Some(root), capabilities).await;
        session
            .notification("$/progress", |p| p["value"]["kind"] == "end")
            .await
            .expect("workspace indexing never ended");

        let uri = uri_of(&dir.path().join(file_name));
        // Opened for diagnostic publishing (indexing already parsed it).
        session.did_open(&uri, "specforge", text).await;
        session
            .wait_for_notification("textDocument/publishDiagnostics", 5000)
            .await;
        (session, uri, dir, init)
    }

    async fn write(&mut self, msg: &Value) {
        let body = serde_json::to_string(msg).unwrap();
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.writer.write_all(frame.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    /// The next message from the server, answering server-to-client
    /// requests (registrations, progress tokens) with a null result.
    async fn read(&mut self) -> Value {
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0u8; 1];
            self.reader.read_exact(&mut byte).await.unwrap();
            header.push(byte[0]);
        }
        let header = String::from_utf8(header).unwrap();
        let length: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length:"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut body = vec![0u8; length];
        self.reader.read_exact(&mut body).await.unwrap();
        let msg: Value = serde_json::from_slice(&body).unwrap();
        if msg.get("method").is_some() && msg.get("id").is_some() {
            let reply = json!({"jsonrpc": "2.0", "id": msg["id"], "result": null});
            self.write(&reply).await;
        }
        msg
    }

    /// Send a request and return its response; notifications that arrive
    /// meanwhile are kept for `notification`. A server-to-client request
    /// carries an id of its own, which may equal ours: it is answered,
    /// never taken for our response.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut msg = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if !params.is_null() {
            msg["params"] = params;
        }
        self.write(&msg).await;
        loop {
            let msg = self.read().await;
            if msg.get("method").is_none() && msg["id"] == id {
                return msg;
            }
            if msg.get("method").is_some() {
                self.pending.push(msg);
            }
        }
    }

    pub async fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write(&msg).await;
    }

    /// The params of the first `method` notification (or answered
    /// server-to-client request) matching `pred`, kept or arriving within
    /// `wait`; it is taken, the others kept.
    pub async fn notification_within(
        &mut self,
        method: &str,
        wait: Duration,
        pred: impl Fn(&Value) -> bool,
    ) -> Option<Value> {
        let hit = |m: &Value| m["method"] == method && pred(&m["params"]);
        if let Some(i) = self.pending.iter().position(hit) {
            return Some(self.pending.remove(i)["params"].clone());
        }
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let msg = tokio::time::timeout_at(deadline, self.read()).await.ok()?;
            if hit(&msg) {
                return Some(msg["params"].clone());
            }
            if msg.get("method").is_some() {
                self.pending.push(msg);
            }
        }
    }

    /// Every message the server sends (its notifications, and its requests, answered) until one
    /// matching `last` arrives within `wait`, that one included; what was kept before comes
    /// first. `None` when `last` never comes.
    pub async fn messages_until(
        &mut self,
        wait: Duration,
        last: impl Fn(&Value) -> bool,
    ) -> Option<Vec<Value>> {
        let mut seen = std::mem::take(&mut self.pending);
        if let Some(i) = seen.iter().position(&last) {
            self.pending = seen.split_off(i + 1);
            return Some(seen);
        }
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let msg = tokio::time::timeout_at(deadline, self.read()).await.ok()?;
            if msg.get("method").is_none() {
                continue;
            }
            let done = last(&msg);
            seen.push(msg);
            if done {
                return Some(seen);
            }
        }
    }

    /// [`Self::notification_within`] ten seconds.
    pub async fn notification(
        &mut self,
        method: &str,
        pred: impl Fn(&Value) -> bool,
    ) -> Option<Value> {
        self.notification_within(method, Duration::from_secs(10), pred)
            .await
    }

    /// The next `method` message (the whole message) to arrive within
    /// `timeout_ms`: what was kept before is dropped, and so is every
    /// other message read meanwhile.
    pub async fn wait_for_notification(&mut self, method: &str, timeout_ms: u64) -> Option<Value> {
        self.pending.clear();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let msg = tokio::time::timeout_at(deadline, self.read()).await.ok()?;
            if msg["method"] == method {
                return Some(msg);
            }
        }
    }

    /// The next diagnostics published for `uri`.
    pub async fn diagnostics(&mut self, uri: &str) -> Vec<Value> {
        let params = self
            .notification("textDocument/publishDiagnostics", |p| p["uri"] == uri)
            .await
            .unwrap_or_else(|| panic!("no diagnostics published for {uri}"));
        params["diagnostics"].as_array().unwrap().clone()
    }

    /// `initialize` with no capabilities declared; the whole response.
    pub async fn initialize(&mut self, root_uri: Option<&str>) -> Value {
        self.initialize_with(root_uri, json!({})).await
    }

    /// `initialize` as a client declaring `capabilities`; the whole
    /// response.
    pub async fn initialize_with(&mut self, root_uri: Option<&str>, capabilities: Value) -> Value {
        let root = root_uri.map(|r| {
            tower_lsp::lsp_types::Url::from_file_path(r)
                .unwrap()
                .to_string()
        });
        let params = json!({
            "processId": null,
            "rootUri": root,
            "capabilities": capabilities,
        });
        self.request("initialize", params).await
    }

    pub async fn initialized(&mut self) {
        self.notify("initialized", json!({})).await;
    }

    pub async fn shutdown(&mut self) -> Value {
        self.request("shutdown", json!(null)).await
    }

    pub async fn open(&mut self, uri: &str, text: &str) {
        self.did_open(uri, "specforge", text).await;
    }

    pub async fn close(&mut self, uri: &str) {
        self.did_close(uri).await;
    }

    pub async fn did_open(&mut self, uri: &str, language_id: &str, text: &str) {
        let doc = json!({"uri": uri, "languageId": language_id, "version": 1, "text": text});
        self.notify("textDocument/didOpen", json!({"textDocument": doc}))
            .await;
    }

    pub async fn did_change(&mut self, uri: &str, version: i32, changes: Vec<Value>) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": uri, "version": version},
                "contentChanges": changes,
            }),
        )
        .await;
    }

    pub async fn did_close(&mut self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri}}),
        )
        .await;
    }

    /// A request about a position of `uri`.
    async fn at(&mut self, method: &str, uri: &str, line: u32, character: u32) -> Value {
        self.request(
            method,
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
            }),
        )
        .await
    }

    pub async fn hover(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.at("textDocument/hover", uri, line, character).await
    }

    pub async fn goto_definition(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.at("textDocument/definition", uri, line, character)
            .await
    }

    pub async fn completion(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.at("textDocument/completion", uri, line, character)
            .await
    }

    pub async fn references(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.references_with(uri, line, character, true).await
    }

    /// `textDocument/references`, the declaration included only when
    /// `include_declaration`.
    pub async fn references_with(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Value {
        self.request(
            "textDocument/references",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
                "context": {"includeDeclaration": include_declaration},
            }),
        )
        .await
    }

    pub async fn rename_range_at(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.at("textDocument/prepareRename", uri, line, character)
            .await
    }

    pub async fn rename(&mut self, uri: &str, line: u32, character: u32, new_name: &str) -> Value {
        self.request(
            "textDocument/rename",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
                "newName": new_name,
            }),
        )
        .await
    }

    pub async fn code_action(
        &mut self,
        uri: &str,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> Value {
        self.request(
            "textDocument/codeAction",
            json!({
                "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": start_line, "character": start_char},
                    "end": {"line": end_line, "character": end_char},
                },
                "context": {"diagnostics": []},
            }),
        )
        .await
    }

    pub async fn document_symbol(&mut self, uri: &str) -> Value {
        self.request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri}}),
        )
        .await
    }

    pub async fn workspace_symbol(&mut self, query: &str) -> Value {
        self.request("workspace/symbol", json!({"query": query}))
            .await
    }

    pub async fn semantic_tokens_full(&mut self, uri: &str) -> Value {
        self.request(
            "textDocument/semanticTokens/full",
            json!({"textDocument": {"uri": uri}}),
        )
        .await
    }

    /// `textDocument/formatting` with `tab_size` spaces.
    pub async fn formatting(&mut self, uri: &str, tab_size: u32) -> Value {
        self.request(
            "textDocument/formatting",
            json!({
                "textDocument": {"uri": uri},
                "options": {"tabSize": tab_size, "insertSpaces": true},
            }),
        )
        .await
    }

    /// `textDocument/formatting`'s result, two spaces (it only serves open
    /// documents).
    pub async fn format(&mut self, uri: &str) -> Value {
        self.formatting(uri, 2).await["result"].clone()
    }

    pub async fn range_formatting(
        &mut self,
        uri: &str,
        tab_size: u32,
        start_line: u32,
        end_line: u32,
    ) -> Value {
        self.request(
            "textDocument/rangeFormatting",
            json!({
                "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": start_line, "character": 0},
                    "end": {"line": end_line, "character": 0},
                },
                "options": {"tabSize": tab_size, "insertSpaces": true},
            }),
        )
        .await
    }
}
