use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::sync::{mpsc, watch};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use specforge_project::RuntimeSource;
use specforge_watch::Debouncer;

use crate::changes::Change;
use crate::editor::ClientEditor;
use crate::reaction::Reaction;
use crate::{ClientSupport, LspState, answers};

use specforge_ops::authoring::format;

/// The reaction, behind the queue every change waits in (ADR 0043 D4).
type Shared = Arc<Mutex<Reaction<ClientEditor>>>;

pub struct Backend {
    /// Formatting publishes and logs on the request path (ADR 0043 D8).
    client: Client,
    state: Arc<RwLock<LspState>>,
    /// The workspace root: rootUri, else the first workspace folder (the project it is in opens,
    /// `Reaction::open`).
    root_dir: Arc<Mutex<Option<String>>>,
    /// Latest-wins reparse requests (C4-03): keystrokes send here; one
    /// serialized worker coalesces and processes, so at most one
    /// whole-graph pass runs at a time and the state lock is never held
    /// across a keystroke storm.
    update_tx: mpsc::UnboundedSender<Url>,
    /// What every change to the project session is reacted to by: applied,
    /// published, the editor's watchers followed, its highlighting
    /// refreshed (ADR 0035, ADR 0043).
    reaction: Shared,
    /// Dropped with the backend: the editor's calls give up once the server is gone, so a
    /// request the editor never answers cannot hold the reaction's thread (and the runtime's
    /// shutdown) for good.
    _gone: watch::Sender<()>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let state = Arc::new(RwLock::new(LspState::new()));
        let (gone, closed) = watch::channel(());
        let reaction: Shared = Arc::new(Mutex::new(Reaction::new(
            ClientEditor::new(client.clone(), closed),
            Arc::clone(&state),
            RuntimeSource::project(),
        )));
        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<Url>();

        // Serialized latest-wins reparse worker (C4-03). Exits when the
        // Backend (and its sender) is dropped.
        let worker = Arc::clone(&reaction);
        tokio::spawn(async move {
            // The rule `specforge watch` batches file changes by: the burst
            // is quiet for the debounce window, each document once.
            let debouncer = Debouncer::new(specforge_watch::DEFAULT_DEBOUNCE_WINDOW);
            while let Some(pending) = debouncer.coalesce_async(&mut update_rx).await {
                // Everything the burst edited is one update (ADR 0023 D9).
                react(&worker, Change::Edited(pending)).await;
            }
        });

        Self {
            client,
            state,
            root_dir: Arc::new(Mutex::new(None)),
            update_tx,
            reaction,
            _gone: gone,
        }
    }
}

/// The workspace root: rootUri, else the first workspace folder.
fn workspace_root(params: &InitializeParams) -> Option<String> {
    params
        .root_uri
        .as_ref()
        .and_then(crate::uri::file_path)
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| {
            params
                .workspace_folders
                .as_ref()
                .and_then(|folders| folders.first())
                .and_then(|f| crate::uri::file_path(&f.uri))
                .map(|p| p.to_string_lossy().to_string())
        })
}

/// Run `change` through the reaction on the blocking pool, after every change queued before it.
async fn react(reaction: &Shared, change: Change) {
    let mut reaction = Arc::clone(reaction).lock_owned().await;
    // The reaction reports an update's panic itself; a panic here (a debug build's divergence
    // assertion) has left the state whole, and the next change goes on.
    let _ = tokio::task::spawn_blocking(move || {
        reaction.react(change);
    })
    .await;
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.state
            .write()
            .await
            .set_client(ClientSupport::of(&params.capabilities));
        // The project is opened at this root once initialized: its
        // specforge.json names the spec root and the extensions, read the
        // way `specforge check` reads them.
        *self.root_dir.lock().await = workspace_root(&params);
        Ok(crate::capabilities::initialize_result())
    }

    async fn initialized(&self, _: InitializedParams) {
        let root = self.root_dir.lock().await.clone().map(PathBuf::from);
        // Taken before returning, so every change the client reports next waits for the
        // project to open; the open itself runs in the background with workDone progress
        // (C4-04): `initialized` returns immediately so the session stays responsive.
        let mut reaction = Arc::clone(&self.reaction).lock_owned().await;
        tokio::task::spawn_blocking(move || reaction.open(root));
    }

    async fn shutdown(&self) -> Result<()> {
        self.state.write().await.shutdown();
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.applying("textDocument/didOpen", |state| {
            state.open_document(&document.uri, &document.text, document.version);
        })
        .await;

        react(&self.reaction, Change::Edited(vec![document.uri])).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let id = params.text_document;

        // Apply text changes immediately (keeps buffer current for completions/hover)
        self.applying("textDocument/didChange", |state| {
            state.change_document(&id.uri, id.version, &params.content_changes);
        })
        .await;

        // Hand off to the serialized latest-wins worker (C4-03): the burst
        // is coalesced and one whole-graph pass runs at a time.
        let _ = self.update_tx.send(id.uri);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.applying("textDocument/didClose", |state| state.close_document(&uri))
            .await;
        // The buffer is no longer the truth for its file (ADR 0023 D9): the reaction publishes
        // the file as the project reports it now (ADR 0043 D7).
        react(&self.reaction, Change::Closed(uri)).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        // One reaction for the whole batch: an extension reload (new kind
        // classifications), a deletion or an on-disk edit may all have
        // changed what open editors highlight.
        react(&self.reaction, Change::Watched(params.changes)).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let at = params.text_document_position_params;
        self.answering("textDocument/hover", |state| {
            Ok(answers::hover(state, &at.text_document.uri, at.position))
        })
        .await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let at = params.text_document_position;
        self.answering("textDocument/completion", |state| {
            Ok(answers::completion(
                state,
                &at.text_document.uri,
                at.position,
            ))
        })
        .await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let at = params.text_document_position_params;
        self.answering("textDocument/definition", |state| {
            Ok(answers::definition(
                state,
                &at.text_document.uri,
                at.position,
            ))
        })
        .await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let at = params.text_document_position;
        self.answering("textDocument/references", |state| {
            Ok(answers::references(
                state,
                &at.text_document.uri,
                at.position,
                params.context.include_declaration,
            ))
        })
        .await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        self.answering("textDocument/prepareRename", |state| {
            answers::prepare_rename(state, &params.text_document.uri, params.position)
        })
        .await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let at = params.text_document_position;
        self.answering("textDocument/rename", |state| {
            answers::rename(state, &at.text_document.uri, at.position, &params.new_name)
        })
        .await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.answering("textDocument/codeAction", |state| {
            Ok(answers::code_actions(
                state,
                &params.text_document.uri,
                params.range,
            ))
        })
        .await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.answering("textDocument/documentSymbol", |state| {
            Ok(answers::document_symbols(state, &params.text_document.uri))
        })
        .await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        self.answering("workspace/symbol", |state| {
            Ok(answers::workspace_symbols(state, &params.query))
        })
        .await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.answering("textDocument/semanticTokens/full", |state| {
            Ok(answers::semantic_tokens(state, &params.text_document.uri))
        })
        .await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        self.format(&params.text_document.uri, &params.options, None)
            .await
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let lines = format::Lines {
            first: params.range.start.line as usize,
            last: params.range.end.line as usize,
        };
        self.format(&params.text_document.uri, &params.options, Some(lines))
            .await
    }
}

impl Backend {
    /// What `answer` reads from the state, or -32603 InternalError after telling the client what `method`
    /// panicked with: a panic in a handler is a fault of the server, not the end of the session
    /// (`lsp_survives_its_faults`). `tokio::sync::RwLock` does not poison, so the state is whole after it.
    async fn answering<T>(
        &self,
        method: &str,
        answer: impl FnOnce(&LspState) -> Result<T>,
    ) -> Result<T> {
        let outcome = {
            let state = self.state.read().await;
            crate::unwind::unwinding(|| answer(&state))
        };
        match outcome {
            Ok(result) => result,
            Err(panic) => {
                self.fault(method, &panic).await;
                Err(tower_lsp::jsonrpc::Error::internal_error())
            }
        }
    }

    /// A notification's change to the state: a panic is logged and the server goes on.
    async fn applying(&self, method: &str, apply: impl FnOnce(&mut LspState)) {
        let outcome = {
            let mut state = self.state.write().await;
            crate::unwind::unwinding(|| apply(&mut state))
        };
        if let Err(panic) = outcome {
            self.fault(method, &panic).await;
        }
    }

    /// Tell the client which handler failed and with what.
    async fn fault(&self, method: &str, panic: &str) {
        self.client
            .log_message(
                MessageType::ERROR,
                format!("specforge-lsp: {method} failed: {panic}"),
            )
            .await;
    }

    /// Format an open document as `specforge format` formats its file
    /// (ADR 0021): what [`answers::formatting`] answers, published beside
    /// the compile's diagnostics, and the editor told once per
    /// configuration that the project's wins over `options`.
    async fn format(
        &self,
        uri: &Url,
        options: &FormattingOptions,
        lines: Option<format::Lines>,
    ) -> Result<Option<Vec<TextEdit>>> {
        let formatted = self
            .answering("textDocument/formatting", |state| {
                Ok(answers::formatting(state, uri, options, lines))
            })
            .await?;
        let Some(formatted) = formatted else {
            return Ok(None);
        };
        let (send, notice) = {
            let mut state = self.state.write().await;
            let send = state.publish_formatting(formatted.findings);
            let notice = formatted
                .notice
                .filter(|(configuration, _)| state.first_format_notice(configuration));
            (send, notice)
        };
        if let Some(send) = send {
            self.client
                .publish_diagnostics(send.uri, send.diagnostics, send.version)
                .await;
        }
        if let Some((_, message)) = notice {
            self.client.log_message(MessageType::INFO, message).await;
        }
        Ok(Some(formatted.edits))
    }
}
