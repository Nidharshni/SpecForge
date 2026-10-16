//! Who a command can ask (ADR 0110): the terminal answers a publisher key change (`add`, `update`) and a
//! collector's command (`collect`) when someone can answer.

use crate::outcome::OutputFormat;
use crate::terminal::Terminal;
use specforge_ops::asking::{Asker, Asking, ExpectedFrom, Question};

/// Who a command can ask: `--yes` answers everything; a human-format command whose stdin is a terminal asks
/// it; under `--format json`, or with nobody at stdin, nobody can be asked.
pub(crate) fn of<'t>(yes: bool, format: OutputFormat, terminal: &'t Terminal) -> Asking<'t> {
    let can_answer = format == OutputFormat::Human && terminal.facts().interactive;
    Asking::of(yes, can_answer.then_some(terminal as &dyn Asker))
}

impl Asker for Terminal {
    fn answer(&self, question: &Question<'_>) -> bool {
        match question {
            Question::KeyChange { .. } => self.ask(&key_change(question)),
            // `collect` is handed its asking in the next ticket of the plan.
            Question::RunCommand { .. } => false,
        }
    }
}

/// The key-change question as the terminal shows it (today's bytes).
fn key_change(question: &Question<'_>) -> String {
    let Question::KeyChange {
        package,
        expected,
        offered,
        from,
    } = question
    else {
        return String::new();
    };
    match from {
        ExpectedFrom::UserPin => format!(
            "KEY CHANGE for '{package}': pinned '{expected}' but new package is signed '{offered}'\ntrust the new key and re-pin? [y/N] "
        ),
        ExpectedFrom::Lock => format!(
            "KEY CHANGE for '{package}': specforge.lock records '{expected}' but new package is signed '{offered}'\ntrust the new key and pin it? [y/N] "
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

    fn change(from: ExpectedFrom) -> Question<'static> {
        Question::KeyChange {
            package: "@x/a",
            expected: "k1",
            offered: "k2",
            from,
        }
    }

    #[specforge_test_macros::test(
        behavior = "pin_publisher_key",
        verify = "the CLI asks its terminal about a key change: yes at the prompt consents, and with nobody to ask nothing is asked"
    )]
    fn a_key_change_is_asked_of_the_terminal() {
        let (terminal, capture) = Terminal::captured("y\n", INTERACTIVE);
        assert!((&terminal as &dyn Asker).answer(&change(ExpectedFrom::UserPin)));
        assert_eq!(
            capture.stderr(),
            "KEY CHANGE for '@x/a': pinned 'k1' but new package is signed 'k2'\ntrust the new key and re-pin? [y/N] "
        );

        let (terminal, capture) = Terminal::captured("y\n", INTERACTIVE);
        assert!((&terminal as &dyn Asker).answer(&change(ExpectedFrom::Lock)));
        assert_eq!(
            capture.stderr(),
            "KEY CHANGE for '@x/a': specforge.lock records 'k1' but new package is signed 'k2'\ntrust the new key and pin it? [y/N] "
        );

        let (terminal, _) = Terminal::captured("n\n", INTERACTIVE);
        assert!(!(&terminal as &dyn Asker).answer(&change(ExpectedFrom::UserPin)));

        let (terminal, capture) = Terminal::captured("y\n", Facts::PIPED);
        assert!(!(&terminal as &dyn Asker).answer(&change(ExpectedFrom::UserPin)));
        assert_eq!(capture.stderr(), "", "nobody to ask: nothing is asked");
    }

    #[test]
    fn who_is_asked_is_yes_then_a_terminal_that_can_answer() {
        let who = |yes, format, facts| {
            let (terminal, _) = Terminal::captured("", facts);
            format!("{:?}", of(yes, format, &terminal))
        };

        assert_eq!(who(true, OutputFormat::Human, Facts::PIPED), "Yes");
        assert_eq!(who(true, OutputFormat::Json, INTERACTIVE), "Yes");
        assert_eq!(who(false, OutputFormat::Json, INTERACTIVE), "Nobody");
        assert_eq!(who(false, OutputFormat::Human, Facts::PIPED), "Nobody");
        assert_eq!(who(false, OutputFormat::Human, INTERACTIVE), "Ask");
    }
}
