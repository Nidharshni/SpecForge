//! What the LSP tells the editor after a change (ADR 0043): the seam between the LSP's
//! [`crate::reaction::Reaction`] and its client. [`ClientEditor`] is the production adapter
//! (tower-lsp's client); the crate's tests record what is sent (`tests/recorder.rs`).

use std::future::Future;

use tokio::runtime::Handle;
use tokio::sync::watch;
use tower_lsp::Client;
use tower_lsp::lsp_types::*;

/// What the LSP's reaction tells the editor. Every call returns once the message is sent and,
/// for [`Editor::watch`], answered: the reaction runs off the async runtime and catches up on
/// disk only after the editor has taken the watchers it asked for (ADR 0035 D2).
pub trait Editor {
    /// Show `diagnostics` for `uri` in place of what the editor showed for it, computed against
    /// the open document's `version` (`None`: the file is not open). An empty list clears it.
    fn publish(&mut self, uri: Url, diagnostics: Vec<Diagnostic>, version: Option<i32>);

    /// Report changes to exactly `watchers` from now on (`workspace/didChangeWatchedFiles`), in
    /// place of what the editor was asked to watch before. `Err` when the editor refused them;
    /// what it watches then is none of the server's.
    fn watch(&mut self, watchers: &[FileSystemWatcher]) -> Result<(), String>;

    /// Log `message` at `level` (`window/logMessage`).
    fn log(&mut self, level: MessageType, message: String);

    /// Show a step of the workspace indexing's progress (`$/progress`, work done).
    fn progress(&mut self, step: WorkDone);

    /// Ask the editor to request semantic tokens again (`workspace/semanticTokens/refresh`),
    /// without waiting for its answer: a slow editor never stalls a reaction.
    fn refresh_tokens(&mut self);
}

/// A step of the workspace indexing, as the editor shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkDone {
    /// Indexing started; `title` is what the editor shows.
    Begin { title: String },
    /// Indexing ended; `message` says what it amounted to.
    End { message: Option<String> },
}

/// The progress token of the workspace indexing.
const INDEXING: &str = "specforge-index";

/// The production [`Editor`]: tower-lsp's client. Each call drives the client's future to
/// completion on the async runtime from the blocking thread the reaction runs on, so messages
/// reach the editor in the order the reaction sends them. Calling it from an async task panics
/// (`Handle::block_on`).
///
/// A request the editor never answers would hold the reaction's thread for good, and the
/// runtime's shutdown with it. So every call gives up when the server is gone (its `Backend`
/// dropped, the sender of `gone` with it): a publication is lost, a watch is refused.
pub(crate) struct ClientEditor {
    client: Client,
    runtime: Handle,
    /// Closed when the server is gone.
    gone: watch::Receiver<()>,
    /// Whether the editor holds watchers registered under [`crate::watchers::REGISTRATION_ID`].
    registered: bool,
}

impl ClientEditor {
    /// The editor `client` talks to, driven on the current runtime until the sender of `gone`
    /// is dropped.
    pub(crate) fn new(client: Client, gone: watch::Receiver<()>) -> Self {
        Self {
            client,
            runtime: Handle::current(),
            gone,
            registered: false,
        }
    }
}

/// `call`'s output, driven on `runtime`, or `None` when the server is gone before it completes.
fn run<T>(
    runtime: &Handle,
    gone: &mut watch::Receiver<()>,
    call: impl Future<Output = T>,
) -> Option<T> {
    runtime.block_on(async {
        tokio::select! {
            out = call => Some(out),
            _ = gone.changed() => None,
        }
    })
}

impl Editor for ClientEditor {
    fn publish(&mut self, uri: Url, diagnostics: Vec<Diagnostic>, version: Option<i32>) {
        let call = self.client.publish_diagnostics(uri, diagnostics, version);
        run(&self.runtime, &mut self.gone, call);
    }

    fn watch(&mut self, watchers: &[FileSystemWatcher]) -> Result<(), String> {
        if self.registered {
            let call = self.client.unregister_capability(vec![Unregistration {
                id: crate::watchers::REGISTRATION_ID.into(),
                method: "workspace/didChangeWatchedFiles".into(),
            }]);
            let _ = run(&self.runtime, &mut self.gone, call);
            self.registered = false;
        }
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: watchers.to_vec(),
        };
        let call = self.client.register_capability(vec![Registration {
            id: crate::watchers::REGISTRATION_ID.into(),
            method: "workspace/didChangeWatchedFiles".into(),
            register_options: Some(
                serde_json::to_value(options).expect("watcher options serialize"),
            ),
        }]);
        run(&self.runtime, &mut self.gone, call)
            .ok_or_else(|| "the server is gone".to_string())?
            .map_err(|refused| refused.to_string())?;
        self.registered = true;
        Ok(())
    }

    fn log(&mut self, level: MessageType, message: String) {
        let call = self.client.log_message(level, message);
        run(&self.runtime, &mut self.gone, call);
    }

    fn progress(&mut self, step: WorkDone) {
        let token = NumberOrString::String(INDEXING.into());
        let value = match step {
            WorkDone::Begin { title } => {
                let call = self.client.send_request::<request::WorkDoneProgressCreate>(
                    WorkDoneProgressCreateParams {
                        token: token.clone(),
                    },
                );
                let _ = run(&self.runtime, &mut self.gone, call);
                WorkDoneProgress::Begin(WorkDoneProgressBegin {
                    title,
                    cancellable: None,
                    message: None,
                    percentage: None,
                })
            }
            WorkDone::End { message } => WorkDoneProgress::End(WorkDoneProgressEnd { message }),
        };
        let call = self
            .client
            .send_notification::<notification::Progress>(ProgressParams {
                token,
                value: ProgressParamsValue::WorkDone(value),
            });
        run(&self.runtime, &mut self.gone, call);
    }

    fn refresh_tokens(&mut self) {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let _ = client.semantic_tokens_refresh().await;
        });
    }
}
