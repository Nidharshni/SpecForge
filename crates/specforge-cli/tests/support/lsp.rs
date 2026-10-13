//! A minimal in-process LSP client over memory streams: what the cross-surface tests open a project with.

use std::collections::BTreeMap;

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
    async fn collect(&mut self, first: Duration, quiet: Duration) -> Published {
        let mut published = Published::new();
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
    pub async fn open(&mut self, path: &std::path::Path) -> Published {
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
    pub async fn watched_file(&mut self, path: &std::path::Path, change: u8) -> Published {
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

/// The last diagnostics published for each URI.
pub type Published = BTreeMap<String, Vec<Value>>;
