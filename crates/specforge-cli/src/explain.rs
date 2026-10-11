//! `specforge explain <CODE>`: print a catalogued diagnostic code
//! ([`specforge_diagnostics`]).

use crate::outcome::{Exit, OutputFormat, Refusal};
use crate::terminal::{Terminal, outln};
use specforge_diagnostics::{CodeEntry, WRAP_WIDTH, lookup, retired, wrap};
use specforge_ops::{OpError, OpErrorKind};

// `specforge explain`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Diagnostic code (e.g., E001, W010)
    code: String,
}

impl Args {
    /// Print the explanation of a diagnostic code.
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        explain(&self.code, terminal)
    }
}

fn explain(code: &str, terminal: &Terminal) -> Exit {
    if let Some(replacement) = retired(code) {
        let old = code.to_uppercase();
        match replacement.and_then(lookup) {
            Some(entry) => {
                outln!(
                    terminal,
                    "{old} is retired; it was renumbered to {}.\n",
                    entry.code
                );
                print_entry(entry, terminal);
            }
            None => outln!(terminal, "{old} is retired and no longer emitted."),
        }
        return Exit::Passed;
    }
    match lookup(code) {
        Some(entry) => {
            print_entry(entry, terminal);
            Exit::Passed
        }
        None => Refusal::of(OutputFormat::Human).report(
            terminal,
            &OpError::new(
                OpErrorKind::InvalidInput,
                "unknown_code",
                format!("unknown diagnostic code: {code}"),
            )
            .with_suggestion(
                "codes are E### (error), W### (warning), I### (info), A### (analyze finding), \
                 R### and R-<AREA>-### (registry client); E900-E998, W900-W998 and I900-I998 are \
                 third-party extensions' (see the extension's documentation)",
            ),
        ),
    }
}

/// The entry, its code bold only where stdout is coloured.
fn print_entry(entry: &CodeEntry, terminal: &Terminal) {
    if terminal.facts().color_out {
        outln!(terminal, "\x1b[1m{}\x1b[0m: {}\n", entry.code, entry.title);
    } else {
        outln!(terminal, "{}: {}\n", entry.code, entry.title);
    }
    outln!(terminal, "{}", wrap(entry.explanation, WRAP_WIDTH));
    outln!(terminal, "\nOwner: {}", entry.owner);
    outln!(terminal, "Level: {}", entry.level.describe());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;

    #[specforge_test_macros::test(
        behavior = "print_diagnostics_structured",
        verify = "specforge explain bolds the code only where its output is coloured"
    )]
    fn explain_bolds_the_code_only_where_stdout_is_coloured() {
        let coloured = Facts {
            color_out: true,
            ..Facts::PIPED
        };
        let (terminal, capture) = Terminal::captured("", coloured);
        let exit = Args {
            code: "e001".into(),
        }
        .run(&terminal);
        assert_eq!(exit, Exit::Passed);
        assert!(
            capture
                .stdout()
                .starts_with("\x1b[1mE001\x1b[0m: Parse error\n"),
            "{:?}",
            capture.stdout()
        );

        let (terminal, capture) = Terminal::captured("", Facts::PIPED);
        let exit = Args {
            code: "e001".into(),
        }
        .run(&terminal);
        assert_eq!(exit, Exit::Passed);
        assert!(
            capture.stdout().starts_with("E001: Parse error\n"),
            "{:?}",
            capture.stdout()
        );
    }
}
