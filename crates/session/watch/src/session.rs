use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use specforge_project::{Changes, ProjectSession, Update, WatchRoot};

use crate::{Debouncer, Follower, Following, SpecWatcher};

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
    following: Following<Vec<WatchRoot>>,
}

impl<W: Watchers> SessionWatch<W> {
    /// Watch what `session` is built from. `Err` when it is built from no
    /// directory ("failed to watch directory: <root> does not exist") or a
    /// watcher cannot start.
    pub fn start(session: ProjectSession, mut watchers: W) -> Result<Self, String> {
        let roots = session.inputs().watch_roots();
        arm(&mut watchers, &session, &roots)?;
        Ok(Self {
            session,
            watchers,
            following: Following::new(roots),
        })
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
            let moved = update.inputs_changed;
            events.push(applied(&self.session, update, changed));
            if moved {
                self.follow(&mut events);
            }
        }
        events
    }

    /// Apply `batch` (changed paths, absolute) and everything that follows
    /// from it. When the update moved what the session is built from, the
    /// watchers follow, then the session catches up on what changed while
    /// they did not (`ensure_fresh`), and again while that catch-up moves it
    /// ([`Following`]). Empty when the batch changes nothing the session is
    /// built from.
    pub fn changed(&mut self, batch: &[PathBuf]) -> Vec<WatchEvent> {
        let changes = self.session.changes(batch.iter().map(PathBuf::as_path));
        let changed = self.labels(&changes);
        let Some(update) = self.session.apply(&changes) else {
            return Vec::new();
        };
        let moved = update.inputs_changed;
        let mut events = vec![applied(&self.session, update, changed)];
        if moved {
            self.follow(&mut events);
        }
        events
    }

    /// Follow the session's inputs: the watchers move, then the session
    /// catches up on what changed while they did not. A failed move is
    /// reported once per set of roots and the catch-up still runs (what is
    /// watched is what was; the next batch tries again).
    fn follow(&mut self, events: &mut Vec<WatchEvent>) {
        self.following.follow(&mut Rounds {
            session: &mut self.session,
            watchers: &mut self.watchers,
            events,
        });
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

/// The session, its watchers and the events of one follow: what [`Following`] moves.
struct Rounds<'a, W> {
    session: &'a mut ProjectSession,
    watchers: &'a mut W,
    events: &'a mut Vec<WatchEvent>,
}

impl<W: Watchers> Follower for Rounds<'_, W> {
    type Watched = Vec<WatchRoot>;

    fn wanted(&self) -> Option<Vec<WatchRoot>> {
        Some(self.session.inputs().watch_roots())
    }

    fn watch(&mut self, wanted: &Vec<WatchRoot>) -> Result<(), String> {
        arm(self.watchers, self.session, wanted)
    }

    fn refused(&mut self, reason: String, was: Vec<WatchRoot>) -> Vec<WatchRoot> {
        self.events.push(WatchEvent::Unwatched(reason));
        was
    }

    fn catch_up(&mut self) -> bool {
        let Some(update) = self.session.ensure_fresh() else {
            return false;
        };
        let changed = update.rebuilt_files.clone();
        let moved = update.inputs_changed;
        self.events.push(applied(self.session, update, changed));
        moved
    }
}

/// Watch `roots`; `Err` when there are none (the session is built from no directory) or the
/// watchers refuse them.
fn arm<W: Watchers>(
    watchers: &mut W,
    session: &ProjectSession,
    roots: &[WatchRoot],
) -> Result<(), String> {
    if roots.is_empty() {
        return Err(format!(
            "failed to watch directory: {} does not exist",
            session.project().environment().root().display()
        ));
    }
    watchers.watch(roots)
}

fn applied(session: &ProjectSession, update: Update, changed: Vec<String>) -> WatchEvent {
    WatchEvent::Applied(Box::new(Applied {
        update,
        changed,
        files: session.project().file_count(),
        nodes: session.project().graph().node_count(),
        extensions: session
            .project()
            .environment()
            .registries()
            .declarations()
            .iter()
            .map(|d| d.name().to_string())
            .collect(),
    }))
}
