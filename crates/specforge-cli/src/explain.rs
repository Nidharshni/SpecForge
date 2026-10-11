//! `specforge explain <CODE>`: print a catalogued diagnostic code
//! ([`specforge_diagnostics`]).

use crate::terminal::{Terminal, outln};

use crate::outcome::{Exit, OutputFormat, Refusal};
use specforge_diagnostics::{CodeEntry, WRAP_WIDTH, lookup, retired, wrap};
use specforge_ops::{OpError, OpErrorKind};

/// Print the explanation of a diagnostic code.
pub fn run(code: &str, terminal: &Terminal) -> Exit {
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

fn print_entry(entry: &CodeEntry, terminal: &Terminal) {
    outln!(terminal, "\x1b[1m{}\x1b[0m: {}\n", entry.code, entry.title);
    outln!(terminal, "{}", wrap(entry.explanation, WRAP_WIDTH));
    outln!(terminal, "\nOwner: {}", entry.owner);
    outln!(terminal, "Level: {}", entry.level.describe());
}
