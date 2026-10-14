use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use specforge_project::{Changes, ProjectSession, Update, WatchRoot};

use crate::{Debouncer, SpecWatcher};

/// How many times a [`SessionWatch`] follows the session's inputs after one
/// batch: each round needs the previous round's catch-up to have moved them
/// again, so it ends long before this in practice.
const FOLLOW_ROUNDS: usize = 8;

/// Where a [`SessionWatch`] watches: the seam between the loop and the file
/// system. [`Notify`] is the production adapter; the crate's tests record
/// what they are asked to watch.
pub trait Watchers {
    /// Watch exactly `roots` (never empty) from now on. The watchers they
    /// replace stop only once the new ones run, so no change falls between
    /// them.
    fn watch(&mut self, roots: &[WatchRoot]) -> Result<(), String>;
}

/// The production [`Watchers`]: one [`SpecWatcher`] per root, every path
/// they report debounced as one stream ([`Debouncer`]), so one burst is one
/// batch however many roots it touches.
pub struct Notify {
    paths: mpsc::Sender<PathBuf>,
    live: Vec<SpecWatcher>,
}

impl Notify {
    /// Watchers whose batches (each `window` after the last change under
    /// any root) arrive on the returned receiver.
    pub fn new(window: Duration) -> (Self, mpsc::Receiver<Vec<PathBuf>>) {
        let (paths, raw) = mpsc::channel::<PathBuf>();
        let (batches_tx, batches) = mpsc::channel();
        // Ends when every sender of `raw` (`Notify` and its watchers'
        // mapping threads) is dropped, or the batches' receiver is.
        std::thread::spawn(move || {
            let debouncer = Debouncer::new(window);
            while let Some(batch) = debouncer.coalesce(&raw) {
                if batches_tx.send(batch).is_err() {
                    return;
                }
            }
        });
        (
            Self {
                paths,
                live: Vec::new(),
            },
            batches,
        )
    }
}

impl Watchers for Notify {
    fn watch(&mut self, roots: &[WatchRoot]) -> Result<(), String> {
        let next = roots
            .iter()
            .map(|root| {
                let watch = if root.recursive {
                    SpecWatcher::new
                } else {
                    SpecWatcher::shallow
                };
                watch(&root.dir, self.paths.clone())
            })
            .collect::<Result<Vec<_>, _>>()?;
        // The new watchers run; only now do the old ones stop.
        self.live = next;
        Ok(())
    }
}

/// What a batch amounted to, in order.
#[derive(Debug)]
pub enum WatchEvent {
    /// An update was applied: the batch's, or a catch-up's.
    Applied(Box<Applied>),
    /// The watchers could not follow what the session is built from; what
    /// is watched is what was.
    Unwatched(String),
}

/// An applied update, with what an event about it names.
#[derive(Debug)]
pub struct Applied {
    pub update: Update,
    /// The sources that changed, by key, and the paths of the batch that
    /// reached an environment or check input that changed, under the project
    /// root (absolute outside it); for a catch-up, the files it rebuilt.
    pub changed: Vec<String>,
    /// The session as the update left it: its files and nodes, and the
    /// extensions it loads, by name.
    pub files: usize,
    pub nodes: usize,
    pub extensions: Vec<String>,
}

/// A project session kept current from file watchers: what `specforge
/// watch` runs (behavior `follow_session_inputs`).
pub struct SessionWatch<W: Watchers> {
    session: ProjectSession,
    watchers: W,
    roots: Vec<WatchRoot>,
}

impl<W: Watchers> SessionWatch<W> {
    /// Watch what `session` is built from. `Err` when it is built from no
    /// directory ("failed to watch directory: <root> does not exist") or a
    /// watcher cannot start.
    pub fn start(session: ProjectSession, watchers: W) -> Result<Self, String> {
        let mut watch = Self {
            session,
            watchers,
            roots: Vec::new(),
        };
        let roots = watch.session.inputs().watch_roots();
        watch.arm(roots)?;
        Ok(watch)
    }

    pub fn session(&self) -> &ProjectSession {
        &self.session
    }

    /// Bring the session up to date with what was written between its open
    /// and its watchers (ADR 0030), following its inputs as they move. Run
    /// before announcing readiness.
    pub fn catch_up(&mut self) -> Vec<WatchEvent> {
        let mut events = Vec::new();
        if let Some(update) = self.session.ensure_fresh() {
            let changed = update.rebuilt_files.clone();
            events.push(self.applied(update, changed));
            self.follow(&mut events);
        }
        events
    }

    /// Apply `batch` (changed paths, absolute) and everything that follows
    /// from it. When the update moved what the session is built from, the
    /// watchers follow, then the session catches up on what changed while
    /// they did not (`ensure_fresh`), and again while that catch-up moves it,
    /// at most `FOLLOW_ROUNDS` times. Empty when the batch changes nothing
    /// the session is built from.
    pub fn changed(&mut self, batch: &[PathBuf]) -> Vec<WatchEvent> {
        let changes = self.session.changes(batch.iter().map(PathBuf::as_path));
        let changed = self.labels(&changes);
        let Some(update) = self.session.apply(&changes) else {
            return Vec::new();
        };
        let mut events = vec![self.applied(update, changed)];
        self.follow(&mut events);
        events
    }

    /// Follow the session's inputs while the last update moved them: the
    /// watchers move, then the session catches up on what changed while
    /// they did not. A failed move is reported once per set of roots and the
    /// catch-up still runs (what is watched is what was; the next batch
    /// tries again).
    fn follow(&mut self, events: &mut Vec<WatchEvent>) {
        let mut refused: Option<Vec<WatchRoot>> = None;
        for _ in 0..FOLLOW_ROUNDS {
            let Some(WatchEvent::Applied(last)) = events.last() else {
                return;
            };
            if !last.update.inputs_changed {
                return;
            }
            let roots = self.session.inputs().watch_roots();
            if roots == self.roots || refused.as_ref() == Some(&roots) {
                return; // every input is under a watched directory, or none can be
            }
            if let Err(e) = self.arm(roots.clone()) {
                refused = Some(roots);
                events.push(WatchEvent::Unwatched(e));
            }
            let Some(update) = self.session.ensure_fresh() else {
                return;
            };
            let changed = update.rebuilt_files.clone();
            events.push(self.applied(update, changed));
        }
    }

    /// Watch `roots`; they are what is watched once the watchers run.
    fn arm(&mut self, roots: Vec<WatchRoot>) -> Result<(), String> {
        if roots.is_empty() {
            return Err(format!(
                "failed to watch directory: {} does not exist",
                self.session.project().environment().root().display()
            ));
        }
        self.watchers.watch(&roots)?;
        self.roots = roots;
        Ok(())
    }

    fn applied(&self, update: Update, changed: Vec<String>) -> WatchEvent {
        WatchEvent::Applied(Box::new(Applied {
            update,
            changed,
            files: self.session.project().file_count(),
            nodes: self.session.project().graph().node_count(),
            extensions: self
                .session
                .project()
                .environment()
                .registries()
                .declarations()
                .iter()
                .map(|d| d.name().to_string())
                .collect(),
        }))
    }

    /// How the changes of a batch are named in its event: a source by its
    /// key under the spec root, any other input by its path under the
    /// project root (absolute outside it).
    fn labels(&self, changes: &Changes) -> Vec<String> {
        let root = std::fs::canonicalize(self.session.project().environment().root())
            .unwrap_or_else(|_| self.session.project().environment().root().to_path_buf());
        let mut labels: Vec<String> = changes
            .environment
            .iter()
            .chain(&changes.check_inputs)
            .map(|path| {
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned()
            })
            .chain(changes.sources.iter().cloned())
            .collect();
        labels.sort();
        labels.dedup();
        labels
    }
}
