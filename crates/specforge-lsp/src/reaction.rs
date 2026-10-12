//! The LSP's one reaction to a change the client reports (ADR 0035 D6, ADR 0043): apply it to
//! the project session, publish what the project reports, then, when what the session is built
//! from moved, ask the editor to watch it and catch up on what changed on disk while it did not,
//! and last tell the editor its highlighting may be stale. Opening the workspace and closing a
//! document are reactions too. Everything is told to the editor through the [`Editor`] port.
//!
//! The reaction is synchronous (ADR 0043, amending ADR 0023 D8): it reads files and runs the
//! checks, so it runs on the blocking pool, one change at a time. It takes the state's lock
//! only briefly, with the blocking accessors, and never holds it while it talks to the editor;
//! readers meanwhile see the session's last complete graph (`LspState`'s stand-in). It must not
//! be called from an async task.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::lsp_types::*;

use specforge_project::{ProjectSession, RuntimeSource};

use crate::LspState;
use crate::changes::{Applied, Change, Plan};
use crate::editor::{Editor, WorkDone};

/// How many times the editor's watchers follow the session after one change: each round needs
/// the previous catch-up to have moved what the session is built from again, so it ends long
/// before this in practice.
const FOLLOW_ROUNDS: usize = 8;

/// What every change to the project session is reacted to by. The backend holds one behind an
/// async mutex, which is the queue changes wait in (ADR 0043 D4).
pub struct Reaction<E> {
    editor: E,
    state: Arc<RwLock<LspState>>,
    /// Where an opened project's extension runtime comes from: the project's own component
    /// runtime in production (`RuntimeSource::project()`), the in-process runtime in tests.
    runtime: RuntimeSource,
    /// The watchers the editor holds for the server ([`crate::watchers`]).
    watched: Vec<FileSystemWatcher>,
}

impl<E: Editor> Reaction<E> {
    /// A reaction to the changes of the session `state` holds, told to `editor`; a project it
    /// opens gets its runtime from `runtime`.
    pub fn new(editor: E, state: Arc<RwLock<LspState>>, runtime: RuntimeSource) -> Self {
        Self {
            editor,
            state,
            runtime,
            watched: Vec::new(),
        }
    }

    /// The workspace opens (`initialized`) at `root`, or with no root: the project `root` is in
    /// (`specforge_common::ProjectRoot::of`), else nothing opens (a path that does not exist). The editor watches every
    /// `.spec`, config and lock file, is shown the indexing's progress while the project opens
    /// ([`Change::Open`], with everything [`Self::react`] does after it: the project opened
    /// holding the open buffers, the project's diagnostics published, its watchers followed), and is told how
    /// many extensions, kinds and files were loaded.
    pub fn open(&mut self, root: Option<PathBuf>) {
        let defaults = crate::watchers::default_watchers();
        if self.editor.watch(&defaults).is_ok() {
            self.watched = defaults;
        }
        self.editor.progress(WorkDone::Begin {
            title: "specforge: indexing workspace".into(),
        });
        let Some(root) = root else {
            self.editor.log(
                MessageType::INFO,
                "specforge-lsp initialized (no root_uri)".into(),
            );
            self.editor.progress(WorkDone::End { message: None });
            return;
        };
        // A folder inside a project opens that project, as the CLI and MCP find it, spelled as
        // the editor spells its files; a folder in no project opens as given.
        let root = match specforge_common::ProjectRoot::of(&root) {
            Ok(root) => root,
            Err(e) => {
                self.editor.log(
                    MessageType::WARNING,
                    format!("specforge-lsp: no project opened: {e}"),
                );
                self.editor.progress(WorkDone::End { message: None });
                return;
            }
        };
        let opened = self.react(Change::Open(root)).is_some();
        let (ext_count, kind_count, file_count, spec_root) = {
            let st = self.state.blocking_read();
            (
                st.registries().declarations().len(),
                st.kind_registry().len(),
                st.session().map_or(0, |s| s.project().file_count()),
                st.spec_root().to_string_lossy().into_owned(),
            )
        };
        if opened && ext_count > 0 {
            // The lsp_initialized announcement: its payload is the extension and entity kind
            // counts.
            self.editor.log(
                MessageType::INFO,
                format!(
                    "specforge-lsp: loaded {ext_count} extension(s), {kind_count} entity kind(s)"
                ),
            );
        }
        self.editor.log(
            MessageType::INFO,
            format!("specforge-lsp: indexed {file_count} .spec files from {spec_root}"),
        );
        self.editor.progress(WorkDone::End {
            message: Some(format!("{file_count} files")),
        });
    }

    /// Apply `change` to the project session and tell the editor what it did:
    /// - the diagnostics the project reports now are published (a closed document's file
    ///   always, as the project reports it);
    /// - an environment reload is announced;
    /// - when what the session is built from moved, the watchers follow and the session
    ///   catches up ([`Change::CatchUp`]);
    /// - an editor whose highlighting went stale is asked to refresh it.
    ///
    /// `None` when there was nothing to apply, or the update panicked (the session is then lost
    /// for a detached one, and the editor is told).
    pub fn react(&mut self, change: Change) -> Option<Applied> {
        let opens = matches!(change, Change::Open(_));
        let applied = self.apply(change);
        if let Some(applied) = &applied {
            if applied.environment && !opens {
                // The environment loaded again (hardening-plan H4 / R-5): the spec root
                // re-indexed and everything republished.
                let n = self.state.blocking_read().registries().declarations().len();
                self.editor.log(
                    MessageType::INFO,
                    format!(
                        "specforge-lsp: extension environment changed, reloaded {n} extension(s)"
                    ),
                );
            }
            if applied.rewatch() {
                self.follow();
            }
        }
        self.refresh_tokens_if_stale();
        applied
    }

    /// Apply `change` to the session, the one `specforge watch` holds, and publish everything
    /// the project reports now: the diagnostics `specforge check` reports for the same sources
    /// and buffers. What the change asks of the session is [`Plan::of`]'s to say.
    ///
    /// The session does synchronous file reads and whole-graph checks: it is taken out of the
    /// state (a brief write lock), updated without any lock held, and put back. Meanwhile
    /// readers see its last complete graph and environment (C4-05). A project is opened in two
    /// steps, its environment shown to readers between them (ADR 0023 D6).
    fn apply(&mut self, change: Change) -> Option<Applied> {
        let (session, plan) = {
            let mut st = self.state.blocking_write();
            let plan = Plan::of(change, &st)?;
            (st.take_session()?, plan)
        };

        // Opening a project loads its environment first and shows it to readers before the
        // sources are read: the kinds and fields keyword completion offers need no `.spec`
        // file, so they are answered while indexing runs (CONTEXT: Environment).
        let opening = match plan.root().cloned() {
            Some(root) => {
                let runtime = self.runtime.clone();
                match unwinding(move || ProjectSession::begin_open(&root, runtime)) {
                    Ok(opening) => {
                        self.state
                            .blocking_write()
                            .show_environment(Arc::clone(opening.environment()));
                        Some(opening)
                    }
                    Err(panic) => return self.lose_session(panic),
                }
            }
            None => None,
        };

        let updated = unwinding(move || {
            // The project opens in the place of the session the editor's buffers were
            // given to (detached until now): it holds them, built in its one cold build.
            let mut session = match opening {
                Some(opening) => opening.finish_holding(session.into_buffers()),
                None => session,
            };
            let applied = plan.apply(&mut session);
            (session, applied)
        });
        let (session, applied) = match updated {
            Ok(updated) => updated,
            Err(panic) => return self.lose_session(panic),
        };
        self.state.blocking_write().set_session(session);
        // Every session verifies its updates in a debug build (ADR 0032): a rebuild that
        // differs from a cold build is reported here, after the session is back so the state
        // stays whole.
        for divergence in &applied.divergences {
            self.editor.log(
                MessageType::ERROR,
                format!("an incremental rebuild diverged from a cold build: {divergence}"),
            );
        }
        self.publish(&applied);
        debug_assert!(applied.divergences.is_empty(), "{:?}", applied.divergences);
        Some(applied)
    }

    /// An update panicked: the session is lost, so the state falls back to an empty one rather
    /// than a stale stand-in.
    fn lose_session(&mut self, panic: String) -> Option<Applied> {
        self.state
            .blocking_write()
            .set_session(ProjectSession::detached());
        self.editor.log(
            MessageType::ERROR,
            format!("specforge-lsp: recompile failed: {panic}"),
        );
        None
    }

    /// What the session is built from moved: the editor watches it now
    /// ([`Self::sync_watchers`]), then the session catches up on what changed on disk while it
    /// did not ([`Change::CatchUp`]), again while that catch-up moves it (ADR 0035; at most
    /// `FOLLOW_ROUNDS`).
    fn follow(&mut self) {
        for _ in 0..FOLLOW_ROUNDS {
            if !self.sync_watchers() {
                return;
            }
            if !self
                .apply(Change::CatchUp)
                .is_some_and(|applied| applied.rewatch())
            {
                return;
            }
        }
    }

    /// Ask the editor to watch every file the project is built from
    /// ([`crate::watchers::file_watchers`]), in place of what it watched, when the two differ;
    /// an editor that refuses keeps the static watchers. Whether it was asked.
    fn sync_watchers(&mut self) -> bool {
        let wanted = {
            let st = self.state.blocking_read();
            let Some(session) = st.session() else {
                return false;
            };
            crate::watchers::file_watchers(session.inputs(), st.client().relative_patterns)
        };
        if self.watched == wanted {
            return false;
        }
        self.watched = match self.editor.watch(&wanted) {
            Ok(()) => wanted,
            Err(_) => {
                let defaults = crate::watchers::default_watchers();
                let _ = self.editor.watch(&defaults);
                defaults
            }
        };
        true
    }

    /// Publish what `applied` says the project reports now ([`Applied::publication`]). What is
    /// published is kept: code actions act on it.
    fn publish(&mut self, applied: &Applied) {
        let publication = applied.publication(&self.state.blocking_read());
        let Some(publication) = publication else {
            return;
        };
        let sends = self.state.blocking_write().publish(publication);
        for send in sends {
            self.editor
                .publish(send.uri, send.diagnostics, send.version);
        }
    }

    /// After a recompile: when the graph changed in anything semantic tokens depend on (entity
    /// IDs, kinds, titles, the kind registry's classification), ask an editor that declared
    /// refreshSupport to request them again. The LSP does not subscribe to watch deltas; this
    /// is how open editors learn their highlighting went stale.
    fn refresh_tokens_if_stale(&mut self) {
        let (stale, supported) = {
            let mut st = self.state.blocking_write();
            (st.record_token_signature(), st.client().tokens_refresh)
        };
        if stale && supported {
            self.editor.refresh_tokens();
        }
    }
}

/// `f`'s result, or the message it panicked with.
fn unwinding<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|panic| {
        panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "an update panicked".into())
    })
}
