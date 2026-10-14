//! What keeps a project session current from the file system: the file
//! watcher ([`SpecWatcher`]), the one debounce rule watch and the LSP batch
//! changes by ([`Coalescer`], on a std or a tokio channel through
//! [`Debouncer`]), and the loop `specforge watch` runs ([`SessionWatch`]):
//! apply each batch, follow what the session is built from after an update
//! that moved it, catch up on what changed meanwhile (ADR 0035). What a
//! path is to the project, and what its change does, is the session's.

mod coalesce;
mod debounce;
mod session;
mod watcher;

pub use coalesce::{Coalescer, DEFAULT_DEBOUNCE_WINDOW};
pub use debounce::Debouncer;
pub use session::{Applied, Notify, SessionWatch, WatchEvent, Watchers};
pub use watcher::SpecWatcher;
