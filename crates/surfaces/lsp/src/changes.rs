//! What a change the client reports asks of the project session, and what
//! applying it did. The plan is read from the state while the backend holds
//! it; it is applied to the session on the blocking pool with no lock held;
//! what it did names what to publish. A change applies as one update of the
//! session, whatever number of buffers it carries (ADR 0023 D9); what an open
//! buffer is to its file is the session's to say (ADR 0046).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_common::ProjectRoot;
use specforge_project::{
    Buffer, Changes, CheckMode, Environment, ProjectSession, RuntimeSource, Update, UpdateKind,
};
use tower_lsp::lsp_types::{FileEvent, Url};

use crate::LspState;
use crate::publish::Publication;
use crate::uri;

/// A change the client reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The workspace opened at this root (`initialized`): the session is
    /// reopened there, holding its buffers.
    Open(ProjectRoot),
    /// These documents were opened or edited: everything the debounce
    /// coalesced. Each one still open has its buffer as the truth for its
    /// file.
    Edited(Vec<Url>),
    /// This document was closed: its buffer is no longer the truth for its
    /// file.
    Closed(Url),
    /// The client's file watchers reported these events.
    Watched(Vec<FileEvent>),
    /// The client watches what the session is built from now: apply what
    /// changed on disk while it did not (`ProjectSession::stale`, which
    /// leaves out every held buffer).
    CatchUp,
}

/// What a change will apply: exactly one step ([`Plan::of`]).
pub struct Plan(Step);

enum Step {
    /// Reopen the session at this root (`Change::Open`), holding the buffers it held.
    Open(ProjectRoot),
    /// Apply what the watched paths, or the catch-up, found changed on disk; never empty.
    Disk(Changes),
    /// Release the buffer of the document closed at this absolute path.
    Release(PathBuf),
    /// Hold these open buffers (by absolute path, the session keys them; never empty) as one
    /// update. `edited` is the session key of the last one edited, where a diagnostic about no
    /// entity goes.
    Hold {
        buffers: Vec<Buffer>,
        edited: String,
    },
}

impl Step {
    /// The step for changes found on disk; `None` when there are none.
    fn on_disk(changes: Changes) -> Option<Step> {
        (!changes.is_empty()).then_some(Step::Disk(changes))
    }
}

impl Plan {
    /// What `change` asks of the session `state` holds, read from the state
    /// as it is now:
    /// - `Open`: the root; the opened project holds the buffers the session it replaces held;
    /// - `Edited`: the buffers of the documents still open, the checks
    ///   skipped while any of them does not parse (the fast path while the editor types);
    /// - `Closed`: the session releases the buffer (a project source is read
    ///   from disk again; any other file, and every file of a session with no
    ///   project, leaves the project);
    /// - `Watched`: what the paths reach that changed on disk
    ///   (`ProjectSession::changes`, which leaves out every held buffer; a
    ///   reload keeps them);
    /// - `CatchUp`: what the session finds changed on disk since it last
    ///   read it (`ProjectSession::stale`).
    ///
    /// `None` when it asks nothing: no session is held, every edited
    /// document was closed since, the closed document was opened again, no
    /// watched path is an input of the project.
    pub fn of(change: Change, state: &LspState) -> Option<Plan> {
        let session = state.session()?;
        let step = match change {
            Change::Open(root) => Step::Open(root),
            Change::Edited(uris) => {
                // The distinct documents still open, in the order they were last edited.
                let files = state.files();
                let mut open: Vec<String> = Vec::new();
                for uri in &uris {
                    let key = state.key_of(uri);
                    if files.document(&key).is_some() {
                        open.retain(|k| *k != key);
                        open.push(key);
                    }
                }
                let edited = open.last().cloned()?;
                Step::Hold {
                    // The buffer of each, as the path it was opened as.
                    buffers: open
                        .iter()
                        .filter_map(|key| {
                            let document = files.document(key)?;
                            let buffer = Buffer::new(files.path(key)?, document.text());
                            Some(buffer.at_version(document.version()))
                        })
                        .collect(),
                    edited,
                }
            }
            Change::Closed(uri) => {
                if state.document(&uri).is_some() {
                    return None;
                }
                Step::Release(uri::path_of(&uri))
            }
            Change::Watched(events) => {
                let paths: Vec<PathBuf> = events
                    .iter()
                    .map(|event| uri::path_of(&event.uri))
                    .collect();
                Step::on_disk(session.changes(paths.iter().map(PathBuf::as_path)))?
            }
            Change::CatchUp => Step::on_disk(session.stale())?,
        };
        Some(Plan(step))
    }

    /// Apply the step to `session`. Blocking: it reads files and runs the checks. `Open` reopens
    /// the session at its root (`ProjectSession::reopen`: the project's extensions from `runtime`,
    /// its environment handed to `loaded` before any source is read, the session's buffers carried
    /// over); `Disk` applies the changes found on disk (`ProjectSession::apply`); `Release`
    /// releases the closed document's buffer (`ProjectSession::release`); `Hold` holds every edited
    /// buffer as one update (`ProjectSession::hold_with`).
    pub fn apply(
        self,
        session: &mut ProjectSession,
        runtime: RuntimeSource,
        loaded: impl FnOnce(&Arc<Environment>),
    ) -> Applied {
        let mut applied = Applied::default();
        match self.0 {
            Step::Open(root) => applied.opened(session.reopen(&root, runtime, loaded)),
            Step::Disk(changes) => {
                if let Some(update) = session.apply(&changes) {
                    applied.environment |= update.kind == UpdateKind::Environment;
                    applied.record(update);
                    applied.changed = true;
                }
                applied.touched.extend(changes.sources().iter().cloned());
            }
            Step::Release(path) => applied.close(session, &path),
            Step::Hold { buffers, edited } => {
                // The syntax-only fast path (C4-07): no checks while an edited file does not parse.
                let update = session.hold_with(&buffers, CheckMode::SyntaxOnlyIfParseErrors);
                applied.record(update);
                applied.touched.extend(
                    buffers
                        .iter()
                        .map(|buffer| session.source_key(&buffer.path)),
                );
                applied.changed = true;
                applied.edited = Some(edited);
            }
        }
        applied
    }
}

/// What applying a plan did.
#[derive(Debug, Clone, Default)]
pub struct Applied {
    /// The environment was loaded: the project opened, or it reloaded.
    pub environment: bool,
    /// The session changed.
    pub changed: bool,
    /// The session's inputs changed (`Update::inputs_changed`): what the
    /// client is asked to watch follows it, and only it (ADR 0099).
    pub inputs_changed: bool,
    /// Where an incremental rebuild differed from a cold build, when the
    /// session verifies its updates (debug builds).
    pub divergences: Vec<String>,
    edited: Option<String>,
    touched: Vec<String>,
    /// A document was closed: the editor showed its buffer's diagnostics for this file, so the
    /// file is published as the project reports it now, whether or not the session changed.
    closed: bool,
}

impl Applied {
    /// What reopening the session says: the environment loaded, the session changed, and its
    /// inputs and its own check as the update reports them. None of its rebuilt files is
    /// touched: a project opened in a session's place publishes what it reports and what was
    /// published before ([`Publication::of`]); a file never published needs no clearing
    /// (ADR 0109 U1).
    fn opened(&mut self, update: Update) {
        self.environment |= update.kind == UpdateKind::Environment;
        self.changed = true;
        self.inputs_changed |= update.inputs_changed;
        self.divergences
            .extend(update.divergence().map(str::to_string));
    }

    /// What an update says about the session's inputs and its own check.
    fn record(&mut self, update: Update) {
        self.inputs_changed |= update.inputs_changed;
        self.divergences
            .extend(update.divergence().map(str::to_string));
        self.touched.extend(update.rebuilt_files);
    }

    /// A closed document's buffer is released: a project source is read
    /// from disk again (nothing to do when the disk text is what the
    /// project holds), any other file leaves the project. Either way the
    /// file is published as the project reports it now.
    fn close(&mut self, session: &mut ProjectSession, path: &Path) {
        self.closed = true;
        self.touched.push(session.source_key(path));
        if let Some(update) = session.release(std::slice::from_ref(&path.to_path_buf())) {
            self.record(update);
            self.changed = true;
        }
    }

    /// What to publish now ([`Publication::of`]): the last edited document
    /// is where a diagnostic about no entity goes, and every touched file
    /// without diagnostics gets an empty list. `None` when the session did
    /// not change and no document was closed.
    pub fn publication(&self, state: &LspState) -> Option<Publication> {
        if !self.changed && !self.closed {
            return None;
        }
        Some(Publication::of(
            state,
            self.edited.as_deref(),
            &self.touched,
        ))
    }
}
