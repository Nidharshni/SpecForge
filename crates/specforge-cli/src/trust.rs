//! The terminal's answer to a publisher key change (`add` and `update` without `--yes`).

use crate::terminal::Terminal;
use specforge_registry_client::{KeyChange, PinSource};

/// Ask `terminal` about each key change: the pinned and offered keys, then
/// `[y/N]`; nobody to ask is no (blocking on a pipe nobody answers would
/// hang).
pub(crate) fn asker(terminal: &Terminal) -> impl Fn(&KeyChange) -> bool + Send + Sync + 'static {
    let terminal = terminal.clone();
    move |change| terminal.ask(&question(change))
}

/// The question as the terminal shows it (today's bytes).
fn question(change: &KeyChange) -> String {
    match change.source {
        PinSource::User => format!(
            "KEY CHANGE for '{}': pinned '{}' but new package is signed '{}'\ntrust the new key and re-pin? [y/N] ",
            change.package, change.pinned, change.offered
        ),
        PinSource::Lock => format!(
            "KEY CHANGE for '{}': specforge.lock records '{}' but new package is signed '{}'\ntrust the new key and pin it? [y/N] ",
            change.package, change.pinned, change.offered
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;

    const INTERACTIVE: Facts = Facts {
        color_out: false,
        color_err: false,
        interactive: true,
    };

    fn change() -> KeyChange {
        KeyChange {
            package: "@x/a".into(),
            pinned: "k1".into(),
            offered: "k2".into(),
            source: PinSource::User,
        }
    }

    #[specforge_test_macros::test(
        behavior = "pin_publisher_key",
        verify = "the CLI asks its terminal about a key change: yes at the prompt consents, and with nobody to ask nothing is asked"
    )]
    fn a_key_change_is_asked_of_the_terminal() {
        let (terminal, capture) = Terminal::captured("y\n", INTERACTIVE);
        assert!(asker(&terminal)(&change()));
        assert_eq!(
            capture.stderr(),
            "KEY CHANGE for '@x/a': pinned 'k1' but new package is signed 'k2'\ntrust the new key and re-pin? [y/N] "
        );

        let (terminal, capture) = Terminal::captured("y\n", INTERACTIVE);
        let from_the_lock = KeyChange {
            source: PinSource::Lock,
            ..change()
        };
        assert!(asker(&terminal)(&from_the_lock));
        assert_eq!(
            capture.stderr(),
            "KEY CHANGE for '@x/a': specforge.lock records 'k1' but new package is signed 'k2'\ntrust the new key and pin it? [y/N] "
        );

        let (terminal, _) = Terminal::captured("n\n", INTERACTIVE);
        assert!(!asker(&terminal)(&change()));

        let (terminal, capture) = Terminal::captured("y\n", Facts::PIPED);
        assert!(!asker(&terminal)(&change()));
        assert_eq!(capture.stderr(), "", "nobody to ask: nothing is asked");
    }
}
