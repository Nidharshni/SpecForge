//! The terminal's answer to a publisher key change (`add` and `update` without `--yes`).

use crate::terminal::Terminal;
use specforge_registry_client::KeyChange;

/// Ask `terminal` about each key change: the pinned and offered keys, then
/// `[y/N]`. Refusal is the default, and the answer when nobody can be asked:
/// blocking on a pipe nobody answers would hang.
pub(crate) fn asker(terminal: &Terminal) -> impl Fn(&KeyChange) -> bool + Send + Sync + 'static {
    let terminal = terminal.clone();
    move |change| match terminal.prompt(&question(change)) {
        Some(answer) => matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
        None => false,
    }
}

/// The question as the terminal shows it.
fn question(change: &KeyChange) -> String {
    format!(
        "KEY CHANGE for '{}': pinned '{}' but new package is signed '{}'\ntrust the new key and re-pin? [y/N] ",
        change.package, change.pinned, change.offered
    )
}
