//! What the CLI lends every operation it calls (ADR 0124): every path, since its user is already at the file
//! system (ADR 0092 D6), and its terminal to ask when someone can answer (ADR 0110 D5).

use crate::outcome::OutputFormat;
use crate::terminal::Terminal;
use specforge_ops::{Caller, Containment};

/// Every path: the CLI's reach.
static UNBOUNDED: Containment = Containment::unbounded();

/// The caller of one command: `--yes` answers every question; else the terminal, under human output with someone
/// at stdin; else nobody. A command with no `--yes` passes `false`: it has nothing to consent to with it.
pub(crate) fn of<'t>(yes: bool, format: OutputFormat, terminal: &'t Terminal) -> Caller<'t> {
    Caller::new(&UNBOUNDED, crate::asking::of(yes, format, terminal))
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

    #[test]
    fn the_cli_lends_every_path_and_asks_as_asking_says() {
        for (yes, format, facts) in [
            (true, OutputFormat::Human, Facts::PIPED),
            (true, OutputFormat::Json, INTERACTIVE),
            (false, OutputFormat::Json, INTERACTIVE),
            (false, OutputFormat::Human, Facts::PIPED),
            (false, OutputFormat::Human, INTERACTIVE),
            (true, OutputFormat::Human, INTERACTIVE),
        ] {
            let (terminal, _) = Terminal::captured("", facts);
            let caller = of(yes, format, &terminal);
            assert_eq!(caller.containment().root(), None);
            assert_eq!(
                format!("{:?}", caller.asking()),
                format!("{:?}", crate::asking::of(yes, format, &terminal))
            );
        }
    }
}
