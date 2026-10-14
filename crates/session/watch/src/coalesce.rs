use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// The debounce window watch and the LSP batch changes by: short enough to
/// feel live, long enough to collapse an editor's save storm.
pub const DEFAULT_DEBOUNCE_WINDOW: Duration = Duration::from_millis(50);

/// The debounce rule (behavior `debounce_file_changes`): changes that
/// arrive less than `window` apart join one batch, which is due `window`
/// after the last of them; each change is in it once, in order. It reads no
/// clock: its adapters pass the time ([`crate::Debouncer`]), its tests pass
/// any.
#[derive(Debug)]
pub struct Coalescer<T> {
    window: Duration,
    pending: BTreeSet<T>,
    last: Option<Instant>,
}

impl<T: Ord> Coalescer<T> {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            pending: BTreeSet::new(),
            last: None,
        }
    }

    /// `change` arrived at `now`: it joins the pending batch, which is now
    /// due `window` from `now`.
    pub fn push(&mut self, change: T, now: Instant) {
        self.pending.insert(change);
        self.last = Some(now);
    }

    /// When the pending batch is due; `None` while nothing is pending.
    pub fn due(&self) -> Option<Instant> {
        self.last.map(|last| last + self.window)
    }

    /// The pending batch when `now` is at or past its due time, else `None`.
    pub fn take_due(&mut self, now: Instant) -> Option<Vec<T>> {
        if self.due()? <= now {
            self.flush()
        } else {
            None
        }
    }

    /// The pending batch whatever the time (the window passed by the
    /// adapter's timer, or the source closed); `None` when nothing is
    /// pending.
    pub fn flush(&mut self) -> Option<Vec<T>> {
        self.last = None;
        if self.pending.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.pending).into_iter().collect())
    }
}
