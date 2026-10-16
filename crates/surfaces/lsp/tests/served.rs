//! A project on disk as the LSP serves it, without a client: its files in a temporary
//! directory, its extensions declared with the SDK and run in process (ADR 0025) or the
//! project's own, every change applied by the LSP's own `Reaction` over a `Recorder` of what it
//! tells the editor, every request answered by `specforge_lsp::answers`. For `#[test]`s only:
//! the reaction takes the state's blocking locks, which panic inside an async task.

use std::path::Path;
use std::sync::Arc;

use specforge_extension_sdk::prelude::*;
use specforge_lsp::changes::{Applied, Change};
use specforge_lsp::files::{Findings, Publish};
use specforge_lsp::reaction::Reaction;
use specforge_lsp::{ClientSupport, LspState, answers};
use specforge_project::{RuntimeSource, SharedRuntime};
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;
use tokio::sync::{RwLock, RwLockReadGuard};
use tower_lsp::lsp_types::{HoverContents, Position, Url};

use crate::recorder::{Recorder, Sent};

type Declare = Arc<dyn Fn(&mut ContributionsBuilder) + Send + Sync>;

/// Where the project's extensions run.
enum Runtime {
    /// The extensions declared with [`Served::extension`], in process; `open` writes the config.
    InProcess,
    /// The project's own, as `specforge-lsp` runs them (`RuntimeSource::project()`).
    Project,
}

pub struct Served {
    dir: TempDir,
    state: Arc<RwLock<LspState>>,
    editor: Recorder,
    reaction: Option<Reaction<Recorder>>,
    runtime: Runtime,
    extensions: Vec<(String, Declare)>,
    /// The runtime serving the declared extensions, built when the reaction is: its recorded
    /// calls are what the project ran.
    served: Option<Arc<InProcessRuntime>>,
    opening: Vec<Sent>,
}

impl Served {
    fn with(dir: TempDir, state: LspState, runtime: Runtime) -> Served {
        Served {
            dir,
            state: Arc::new(RwLock::new(state)),
            editor: Recorder::default(),
            reaction: None,
            runtime,
            extensions: Vec::new(),
            served: None,
            opening: Vec::new(),
        }
    }

    /// A project holding `files` (relative path, text); its `specforge.json` enables the
    /// extensions added next.
    pub fn new(files: &[(&str, &str)]) -> Served {
        let dir = TempDir::new().unwrap();
        for (name, text) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        Served::with(dir, LspState::new(), Runtime::InProcess)
    }

    /// The project in `dir` as written (its own `specforge.json`), its extensions run by the
    /// component runtime: builtins, the fixture extensions.
    pub fn at(dir: TempDir) -> Served {
        Served::with(dir, LspState::new(), Runtime::Project)
    }

    /// A server whose workspace has no root, opened (`Reaction::open(None)`).
    pub fn detached() -> Served {
        let mut served = Served::over(LspState::new());
        served.reaction().open(None);
        served.opening = served.editor.take();
        served
    }

    /// `state` held by a server with no root, not opened again: a fixture over an existing
    /// state.
    pub fn over(state: LspState) -> Served {
        Served::with(TempDir::new().unwrap(), state, Runtime::InProcess)
    }

    /// Serve extension `name`, declared by `declare` (the runtime builds it per call).
    pub fn extension(
        mut self,
        name: &str,
        declare: impl Fn(&mut ContributionsBuilder) + Send + Sync + 'static,
    ) -> Served {
        self.extensions.push((name.to_string(), Arc::new(declare)));
        self
    }

    /// The client declared `support` at initialize.
    pub fn client(self, support: ClientSupport) -> Served {
        self.state.blocking_write().set_client(support);
        self
    }

    /// The editor the reaction talks to: refuse its watchers, act while they move.
    pub fn editor(&self) -> &Recorder {
        &self.editor
    }

    /// The runtime every declared extension is served by, in process.
    fn in_process(&self) -> Arc<InProcessRuntime> {
        let mut runtime = InProcessRuntime::new();
        for (name, declare) in &self.extensions {
            let (name, declare) = (name.clone(), Arc::clone(declare));
            runtime = runtime.with(move || {
                let mut c = ContributionsBuilder::new(ExtensionMeta::new(&name, "1.0.0"));
                declare(&mut c);
                c
            });
        }
        Arc::new(runtime)
    }

    /// How many times the project's checks ran: each run calls every check pass once, and a
    /// counting extension declares one.
    pub fn check_runs(&self) -> usize {
        self.served.as_ref().map_or(0, |runtime| {
            runtime
                .calls()
                .iter()
                .filter(|call| call.export.starts_with("__pass_"))
                .count()
        })
    }

    /// The reaction, built on first use with the runtime the project's extensions run in.
    fn reaction(&mut self) -> &mut Reaction<Recorder> {
        if self.reaction.is_none() {
            let source = match self.runtime {
                Runtime::InProcess if self.extensions.is_empty() => RuntimeSource::Fixed(None),
                Runtime::InProcess => {
                    let runtime = self.in_process();
                    self.served = Some(Arc::clone(&runtime));
                    RuntimeSource::Fixed(Some(runtime as SharedRuntime))
                }
                Runtime::Project => RuntimeSource::project(),
            };
            self.reaction = Some(Reaction::new(
                self.editor.clone(),
                Arc::clone(&self.state),
                source,
            ));
        }
        self.reaction.as_mut().expect("just built")
    }

    /// Open the workspace at the project's root as `initialized` does (`Reaction::open`), then
    /// `files` as documents with their disk text (one `Change::Edited`, as a debounced burst).
    /// What that sent is [`Self::opening`]; [`Self::sent`] starts empty after it.
    pub fn open(self, files: &[&str]) -> Served {
        self.open_in("", files)
    }

    /// [`Self::open`] with the workspace opened at `subdir` of the project, as an editor opened
    /// on a directory inside it.
    pub fn open_in(mut self, subdir: &str, files: &[&str]) -> Served {
        if let Runtime::InProcess = self.runtime {
            let names: Vec<&str> = self.extensions.iter().map(|(n, _)| n.as_str()).collect();
            let config = serde_json::json!({"name": "t", "version": "0.1.0", "extensions": names});
            std::fs::write(self.dir.path().join("specforge.json"), config.to_string()).unwrap();
            // What the in-process runtime serves, the project has installed.
            specforge_installed::testing::install(self.dir.path(), &names);
        }
        let root = match subdir {
            "" => self.dir.path().to_path_buf(),
            subdir => self.dir.path().join(subdir),
        };
        self.reaction().open(Some(root));
        let uris: Vec<Url> = files
            .iter()
            .map(|file| {
                let uri = self.uri(file);
                let text = std::fs::read_to_string(self.dir.path().join(file)).unwrap();
                self.state.blocking_write().open_document(&uri, &text, 1);
                uri
            })
            .collect();
        if !uris.is_empty() {
            self.apply(Change::Edited(uris));
        }
        self.opening = self.editor.take();
        self
    }

    /// Open the workspace again, as a second `initialized` does: the session is reopened in
    /// place. What that sent is appended to [`Self::opening`].
    pub fn open_again(&mut self) {
        let root = self.dir.path().to_path_buf();
        self.reaction().open(Some(root));
        let sent = self.editor.take();
        self.opening.extend(sent);
    }

    /// Documents the editor restores before the project opens (`initialized` has not run yet):
    /// each opened holding `text`, which need not be its file's, and applied as the debounce
    /// applies them. Call before `open`.
    pub fn restore(mut self, files: &[(&str, &str)]) -> Served {
        let uris: Vec<Url> = files
            .iter()
            .map(|(file, text)| {
                let uri = self.uri(file);
                self.state.blocking_write().open_document(&uri, text, 1);
                uri
            })
            .collect();
        self.apply(Change::Edited(uris));
        self
    }

    /// The editor's version of the open document `file` becomes `version`.
    pub fn set_version(&mut self, file: &str, version: i32) {
        let uri = self.uri(file);
        self.state
            .blocking_write()
            .change_document(&uri, version, &[]);
    }

    /// What opening the workspace and its documents sent.
    pub fn opening(&self) -> &[Sent] {
        &self.opening
    }

    /// What was sent since the workspace opened, or since the last call.
    pub fn sent(&self) -> Vec<Sent> {
        self.editor.take()
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn uri(&self, file: &str) -> Url {
        Url::from_file_path(self.dir.path().join(file)).unwrap()
    }

    /// The state, read.
    pub fn state(&self) -> RwLockReadGuard<'_, LspState> {
        self.state.blocking_read()
    }

    /// React to `change` (`Reaction::react`).
    pub fn apply(&mut self, change: Change) -> Option<Applied> {
        self.reaction().react(change)
    }

    /// The buffer of `file` becomes `text`; nothing is compiled (the stale window).
    pub fn type_text(&mut self, file: &str, text: &str) {
        let uri = self.uri(file);
        self.type_into(&uri, text);
    }

    /// The buffer of the document the client opened as `uri` becomes `text`; nothing is
    /// compiled (the stale window).
    pub fn type_into(&mut self, uri: &Url, text: &str) {
        let mut state = self.state.blocking_write();
        let version = state
            .document(uri)
            .and_then(|doc| doc.version())
            .unwrap_or(1);
        state.change_document(uri, version, &[crate::whole(text)]);
    }

    /// The editor changed the document it opened as `uri` to `text` at `version`; nothing is
    /// compiled.
    pub fn change_to(&mut self, uri: &Url, version: i32, text: &str) {
        self.state
            .blocking_write()
            .change_document(uri, version, &[crate::whole(text)]);
    }

    /// What formatting found is recorded (`LspState::publish_formatting`); what to send now.
    pub fn publish_formatting(&mut self, findings: Findings) -> Option<Publish> {
        self.state.blocking_write().publish_formatting(findings)
    }

    /// `type_text`, then `apply(Change::Edited([file]))`.
    pub fn edit(&mut self, file: &str, text: &str) -> Option<Applied> {
        self.type_text(file, text);
        let uri = self.uri(file);
        self.apply(Change::Edited(vec![uri]))
    }

    /// The handler half of `didOpen` for `file`, which is read from disk: the document is held
    /// as open, and nothing is reacted to yet (the reaction queues behind the open).
    pub fn hold(&self, file: &str) {
        let uri = self.uri(file);
        let text = std::fs::read_to_string(self.dir.path().join(file)).unwrap();
        self.state.blocking_write().open_document(&uri, &text, 1);
    }

    /// As `didOpen`: `uri` opened with `text` at version 1, then `Change::Edited([uri])`.
    pub fn open_document(&mut self, uri: &Url, text: &str) -> Option<Applied> {
        self.state.blocking_write().open_document(uri, text, 1);
        self.apply(Change::Edited(vec![uri.clone()]))
    }

    /// As `didClose`: `LspState::close_document`, then `Change::Closed`.
    pub fn close(&mut self, file: &str) -> Option<Applied> {
        let uri = self.uri(file);
        self.close_uri(&uri)
    }

    pub fn close_uri(&mut self, uri: &Url) -> Option<Applied> {
        self.state.blocking_write().close_document(uri);
        self.apply(Change::Closed(uri.clone()))
    }

    /// Write `text` at `file` on disk (no event).
    pub fn write(&self, file: &str, text: &str) {
        let path = self.dir.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The hover markdown at a position.
    pub fn hover(&self, file: &str, line: u32, character: u32) -> Option<String> {
        hover_text(
            &self.state(),
            &self.uri(file),
            Position::new(line, character),
        )
    }

    /// The position of the declaration of `id` in `file` (on disk): the first line whose second
    /// word is `id`, one character into the name.
    pub fn position_of(&self, file: &str, id: &str) -> Position {
        let text = std::fs::read_to_string(self.dir.path().join(file)).unwrap();
        text.lines()
            .enumerate()
            .find_map(|(n, line)| {
                let mut words = line.split_whitespace();
                words.next()?;
                (words.next()? == id)
                    .then(|| Position::new(n as u32, line.find(id).unwrap() as u32 + 1))
            })
            .unwrap_or_else(|| panic!("no declaration of {id} in {file}"))
    }

    /// The hover on the declaration of `id` in `file`: the first line whose second word is `id`.
    /// `None` when no line declares it.
    pub fn hover_on(&self, file: &str, id: &str) -> Option<String> {
        let text = std::fs::read_to_string(self.dir.path().join(file)).ok()?;
        text.lines()
            .any(|line| line.split_whitespace().nth(1) == Some(id))
            .then(|| hover_text(&self.state(), &self.uri(file), self.position_of(file, id)))?
    }

    /// `f` over the state while the session is out for an update (its stand-in).
    pub fn rebuilding<R>(&mut self, f: impl FnOnce(&LspState) -> R) -> R {
        let mut state = self.state.blocking_write();
        let session = state.take_session().expect("held");
        let result = f(&state);
        state.set_session(session);
        result
    }

    /// The state, the reaction dropped.
    pub fn into_state(mut self) -> LspState {
        // The reaction holds the state's other `Arc`.
        self.reaction = None;
        Arc::try_unwrap(self.state)
            .ok()
            .expect("the fixture holds the only state")
            .into_inner()
    }
}

/// The markdown of the hover at `position` of the open document `uri`.
pub fn hover_text(state: &LspState, uri: &Url, position: Position) -> Option<String> {
    match answers::hover(state, uri, position)?.contents {
        HoverContents::Markup(markup) => Some(markup.value),
        other => panic!("a hover of markup, not {other:?}"),
    }
}

/// A state with no project holding `files` (absolute path, text) as open buffers, each opened
/// as `didOpen` opens it.
pub fn buffers(files: &[(&str, &str)]) -> LspState {
    let mut served = Served::detached();
    for (path, text) in files {
        served.open_document(&uri_of_path(path), text);
    }
    served.into_state()
}

/// The URI of the file at the absolute `path`.
pub fn uri_of_path(path: &str) -> Url {
    Url::from_file_path(path).unwrap()
}

/// The buffer of the file at the absolute `path` becomes `text` (opened if it was not), applied
/// as the debounce applies it (`Change::Edited`).
pub fn edit_buffer(state: &mut LspState, path: &str, text: &str) {
    let uri = uri_of_path(path);
    let mut served = Served::over(std::mem::take(state));
    {
        let mut held = served.state.blocking_write();
        if held.document(&uri).is_some() {
            held.change_document(&uri, 1, &[crate::whole(text)]);
        } else {
            held.open_document(&uri, text, 1);
        }
    }
    served
        .apply(Change::Edited(vec![uri]))
        .expect("the buffer is open");
    *state = served.into_state();
}
