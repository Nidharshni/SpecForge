//! What a change the client reports asks of the project session, and what
//! applying it did. The plan is read from the state while the backend holds
//! it; it is applied to the session on the blocking pool with no lock held;
//! what it did names what to publish. A change applies as one update of the
//! session, whatever number of buffers it carries (ADR 0023 D9); what an open
//! buffer is to its file is the session's to say (ADR 0046).

use std::path::{Path, PathBuf};

use specforge_common::ProjectRoot;
use specforge_project::{
    Buffer, Changes, CheckMode, ProjectSession, SourceChange, Update, UpdateKind,
};
use tower_lsp::lsp_types::{FileEvent, Url};

use crate::LspState;
use crate::publish::Publication;
use crate::uri;

/// A change the client reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The workspace opened at this root (`initialized`); the project holds
    /// the open buffers.
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

/// What a change will apply.
pub struct Plan {
    /// The project to open (`Change::Open`).
    root: Option<ProjectRoot>,
    /// What the watched paths amount to, applied first.
    disk: Option<Changes>,
    /// The absolute path of the document that was closed: the session
    /// releases its buffer.
    released: Option<PathBuf>,
    /// The open buffers to hold, by absolute path: the session keys them.
    held: Vec<Buffer>,
    /// The session key of the document an anchorless diagnostic goes to.
    edited: Option<String>,
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
    /// - `Watched`: the paths as the session classifies them
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
        let nothing = Plan {
            root: None,
            disk: None,
            released: None,
            held: Vec::new(),
            edited: None,
        };
        match change {
            Change::Open(root) => Some(Plan {
                root: Some(root),
                ..nothing
            }),
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
                Some(Plan {
                    // The buffer of each, as the path it was opened as.
                    held: open
                        .iter()
                        .filter_map(|key| {
                            let document = files.document(key)?;
                            let buffer = Buffer::new(files.path(key)?, document.text());
                            Some(buffer.at_version(document.version()))
                        })
                        .collect(),
                    edited: Some(edited),
                    ..nothing
                })
            }
            Change::Closed(uri) => {
                if state.document(&uri).is_some() {
                    return None;
                }
                Some(Plan {
                    released: Some(uri::path_of(&uri)),
                    ..nothing
                })
            }
            Change::Watched(events) => {
                let paths: Vec<PathBuf> = events
                    .iter()
                    .map(|event| uri::path_of(&event.uri))
                    .collect();
                Plan::on_disk(session.changes(paths.iter().map(PathBuf::as_path)))
            }
            Change::CatchUp => Plan::on_disk(session.stale()),
        }
    }

    /// The plan for changes found on disk, applied first; `None` when there
    /// are none.
    fn on_disk(changes: Changes) -> Option<Plan> {
        if changes.is_empty() {
            return None;
        }
        Some(Plan {
            root: None,
            disk: Some(changes),
            released: None,
            held: Vec::new(),
            edited: None,
        })
    }

    /// The root a project opens at (`Change::Open`). The caller opens it
    /// (`ProjectSession::begin_open`, showing its environment to readers,
    /// then `OpeningProject::finish`) and passes the opened session to
    /// [`Plan::apply`].
    pub fn root(&self) -> Option<&ProjectRoot> {
        self.root.as_ref()
    }

    /// Apply the plan to `session`. Blocking: it reads files and runs the
    /// checks. The watched changes first (`ProjectSession::apply`), then the
    /// released buffer (`ProjectSession::release`), then every buffer as one
    /// update (`SourceChange::Hold`).
    pub fn apply(self, session: &mut ProjectSession) -> Applied {
        let opened = self.root.is_some();
        let mut applied = Applied {
            environment: opened,
            changed: opened,
            inputs_changed: opened,
            divergences: Vec::new(),
            edited: self.edited,
            touched: Vec::new(),
            closed: false,
        };
        if let Some(changes) = &self.disk {
            if let Some(update) = session.apply(changes) {
                applied.environment |= update.kind == UpdateKind::Environment;
                applied.record(update);
                applied.changed = true;
            }
            applied.touched.extend(changes.sources.iter().cloned());
        }
        if let Some(path) = &self.released {
            applied.close(session, path);
        }
        if !self.held.is_empty() {
            // The syntax-only fast path (C4-07): no checks while an edited
            // file does not parse.
            let update = session.update_with(
                SourceChange::Hold(&self.held),
                CheckMode::SyntaxOnlyIfParseErrors,
            );
            applied.record(update);
            applied.touched.extend(
                self.held
                    .iter()
                    .map(|buffer| session.source_key(&buffer.path)),
            );
            applied.changed = true;
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
    /// client is asked to watch must follow them.
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
    /// What the client is asked to watch must follow (ADR 0035): the
    /// environment was loaded, or the session's inputs changed.
    pub fn rewatch(&self) -> bool {
        self.environment || self.inputs_changed
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
