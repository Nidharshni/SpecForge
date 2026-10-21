//! The real registry server in process, for tests of what reaches a registry over HTTP: the CLI, the HTTP
//! client's contract run, the configured registry's (ADR 0044).
#![allow(
    clippy::unwrap_used,
    reason = "test support behind the `testing` feature: a failed unwrap fails the calling test"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use specforge_registry_wire::{PackageMetadata, RegistryRoot};
use tempfile::TempDir;

use crate::db::PackageVersion;
use crate::state::{AppState, PublishLimits, ReadAccess};
use crate::{auth, handlers};

/// The registry server on a free local port, with an empty store and one publisher token, serving until
/// dropped. It runs on its own runtime: create and drop it outside an async context (a `#[test]`, not a
/// `#[tokio::test]`; those drive `handlers::router` with `oneshot`).
pub struct LocalRegistry {
    root: RegistryRoot,
    token: String,
    state: Arc<AppState>,
    requests: Arc<Mutex<Vec<String>>>,
    stored: Mutex<u32>,
    // Dropped before the data directory: the runtime stops the server first.
    _runtime: tokio::runtime::Runtime,
    _data: TempDir,
}

impl LocalRegistry {
    /// [`LocalRegistry::start_with`] 100 publishes per token and per IP.
    pub fn start() -> Self {
        Self::start_with(PublishLimits {
            per_token: 100,
            per_ip: 100,
            window: Duration::from_secs(60),
        })
    }

    pub fn start_with(limits: PublishLimits) -> Self {
        Self::start_as(limits, ReadAccess::Public)
    }

    /// [`LocalRegistry::start`] for a registry that requires a token to read (`serve --private`).
    pub fn start_private() -> Self {
        Self::start_as(
            PublishLimits {
                per_token: 100,
                per_ip: 100,
                window: Duration::from_secs(60),
            },
            ReadAccess::Token,
        )
    }

    fn start_as(limits: PublishLimits, read_access: ReadAccess) -> Self {
        let data = TempDir::new().expect("a temporary data directory");
        let state =
            Arc::new(AppState::open(data.path(), limits, read_access).expect("an empty registry"));
        let token = auth::create_token(
            &state.database,
            "publisher",
            None,
            "publisher",
            Some(1),
            false,
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = handlers::router(Arc::clone(&state)).layer(axum::middleware::from_fn_with_state(
            Arc::clone(&requests),
            log_request,
        ));

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free local port");
        let root = RegistryRoot::parse(&format!(
            "http://{}",
            listener.local_addr().expect("a bound address")
        ))
        .expect("a local address is a registry root");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("a tokio listener");
            axum::serve(listener, app).await.expect("the server runs");
        });

        LocalRegistry {
            root,
            token,
            state,
            requests,
            stored: Mutex::new(0),
            _runtime: runtime,
            _data: data,
        }
    }

    /// The registry's root, as `specforge.json` names it: `http://127.0.0.1:{port}`.
    pub fn root(&self) -> &RegistryRoot {
        &self.root
    }

    /// A bearer token it accepts for publishing (no scope, a day's expiry).
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The `registries` value naming this registry as the default, alias `local`.
    pub fn config_entry(&self) -> serde_json::Value {
        serde_json::json!([{ "alias": "local", "url": self.root.as_str(), "default_registry": true }])
    }

    /// Store `metadata` and `wasm` as a published version, past the publish checks (signature, magic bytes,
    /// manifest, scope): what a registry may already hold. Versions list in store order. `wasm_url` is
    /// ignored (the server serves its own download path); `sha256` is stored as given.
    pub fn store(&self, metadata: &PackageMetadata, wasm: &[u8]) {
        let published_at = {
            let mut stored = self.stored.lock().unwrap();
            *stored += 1;
            // Strictly increasing, so the version list keeps store order.
            chrono::DateTime::from_timestamp(1_700_000_000 + i64::from(*stored), 0)
                .expect("a timestamp")
                .to_rfc3339()
        };
        let row = PackageVersion {
            name: metadata.name.clone(),
            version: metadata.version.clone(),
            sha256: metadata.sha256.clone(),
            size_bytes: metadata.size_bytes,
            description: metadata.description.clone(),
            keywords: metadata.keywords.join(","),
            publisher: metadata.publisher.clone(),
            published_at,
            signature: metadata.signature.clone(),
            manifest: metadata.manifest.clone(),
        };
        let temp = self
            .state
            .storage
            .store_wasm_temp(&row.name, &row.version, wasm)
            .expect("a blob written");
        self.state
            .database
            .insert_package(&row)
            .expect("a row inserted");
        self.state
            .storage
            .commit_wasm(&row.name, &row.version, &temp)
            .expect("a blob committed");
    }

    /// Every request served so far, oldest first: `"GET /v1/packages/@acme%2Ftool"`.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

async fn log_request(
    State(log): State<Arc<Mutex<Vec<String>>>>,
    request: Request,
    next: Next,
) -> Response {
    log.lock()
        .unwrap()
        .push(format!("{} {}", request.method(), request.uri().path()));
    next.run(request).await
}
