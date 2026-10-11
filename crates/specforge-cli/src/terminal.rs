//! Where a core command writes and whom it asks (ADR 0029 D4b).
//!
//! `main` makes the process's terminal ([`Terminal::process`]) and hands it to
//! the command it runs; a test captures one (`Terminal::captured`). No other
//! module names the process's stdio: the crate denies `clippy::print_stdout`
//! and `clippy::print_stderr`, and only this module calls `std::io::stdout`,
//! `stderr`, `stdin` or `IsTerminal`.

use serde::Serialize;
use specforge_common::Diagnostic;
use std::ffi::OsStr;
use std::fmt;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// One of a terminal's two output streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stream {
    Out,
    Err,
}

/// What a command decides by, fixed when its terminal is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Facts {
    /// What is written to stdout may be coloured (`analyze`, `explain`).
    pub color_out: bool,
    /// What is written to stderr may be coloured (`check`).
    pub color_err: bool,
    /// Stdin is a terminal: someone can answer a prompt.
    pub interactive: bool,
}

impl Facts {
    /// Nothing coloured and nobody to ask: what a pipe gives.
    #[cfg(test)]
    pub(crate) const PIPED: Self = Self {
        color_out: false,
        color_err: false,
        interactive: false,
    };

    /// The process's: each stream's colour by [`color`] over `NO_COLOR`,
    /// `CLICOLOR_FORCE` and whether the stream is a terminal; interactive
    /// when stdin is a terminal.
    fn of_process() -> Self {
        let var = |name| std::env::var_os(name);
        let (no, force) = (var("NO_COLOR"), var("CLICOLOR_FORCE"));
        Self {
            color_out: color(no.as_deref(), force.as_deref(), io::stdout().is_terminal()),
            color_err: color(no.as_deref(), force.as_deref(), io::stderr().is_terminal()),
            interactive: io::stdin().is_terminal(),
        }
    }
}

/// Whether a stream is coloured; the first match wins: `NO_COLOR` set to a
/// non-empty value, never (no-color.org); `CLICOLOR_FORCE` set to a
/// non-empty value other than `0`, always, even into a pipe or a file;
/// otherwise only when the stream is a terminal. JSON output never asks, so
/// it is never coloured.
pub(crate) fn color(no_color: Option<&OsStr>, force: Option<&OsStr>, is_terminal: bool) -> bool {
    fn set(value: Option<&OsStr>) -> Option<&OsStr> {
        value.filter(|v| !v.is_empty())
    }
    if set(no_color).is_some() {
        return false;
    }
    if set(force).is_some_and(|v| v != "0") {
        return true;
    }
    is_terminal
}

/// Where a core command writes and whom it asks: its stdout and stderr, the
/// input a prompt reads, and its [`Facts`]. Clones share the streams, so a
/// prompt handed to an operation (`ConfiguredRegistry::asking`, which wants
/// `Fn + Send + Sync + 'static`) writes where the command writes. Each write
/// locks one stream for that write only: nothing holds a stream across a
/// call, so asking while a command writes cannot deadlock.
#[derive(Clone)]
pub(crate) struct Terminal(Arc<Streams>);

struct Streams {
    out: Mutex<Sink>,
    err: Mutex<Sink>,
    input: Mutex<Box<dyn BufRead + Send>>,
    facts: Facts,
}

/// One output stream: its writer and the name std's panic gives it.
struct Sink {
    writer: Box<dyn Write + Send>,
    label: &'static str,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Sink {
    fn new(writer: impl Write + Send + 'static, label: &'static str) -> Mutex<Self> {
        Mutex::new(Self {
            writer: Box::new(writer),
            label,
        })
    }

    /// A failed write panics as `print!` does.
    fn failed(&self, error: &io::Error) -> ! {
        panic!("failed printing to {}: {error}", self.label)
    }
}

impl Terminal {
    /// A terminal over these streams and facts.
    pub(crate) fn new(
        out: impl Write + Send + 'static,
        err: impl Write + Send + 'static,
        input: impl BufRead + Send + 'static,
        facts: Facts,
    ) -> Self {
        Self(Arc::new(Streams {
            out: Sink::new(out, "stdout"),
            err: Sink::new(err, "stderr"),
            input: Mutex::new(Box::new(input)),
            facts,
        }))
    }

    /// The process's own: its stdout, stderr and stdin, and
    /// [`Facts::of_process`]. `main` makes it once.
    pub(crate) fn process() -> Self {
        Self::new(
            io::stdout(),
            io::stderr(),
            BufReader::new(io::stdin()),
            Facts::of_process(),
        )
    }

    /// A terminal whose output a test reads back from the [`Capture`], whose
    /// prompts read `input`, and which decides by `facts`.
    #[cfg(test)]
    pub(crate) fn captured(input: &str, facts: Facts) -> (Self, Capture) {
        let capture = Capture::default();
        let terminal = Self::new(
            capture.out.clone(),
            capture.err.clone(),
            io::Cursor::new(input.as_bytes().to_vec()),
            facts,
        );
        (terminal, capture)
    }

    /// What this terminal decides by.
    pub(crate) fn facts(&self) -> Facts {
        self.0.facts
    }

    fn sink(&self, stream: Stream) -> &Mutex<Sink> {
        match stream {
            Stream::Out => &self.0.out,
            Stream::Err => &self.0.err,
        }
    }

    /// Write `text` to `stream` as it is (what `out!`/`err!` expand to). A
    /// write that fails panics as `print!` does: `failed printing to
    /// stdout: …`.
    pub(crate) fn write(&self, stream: Stream, text: fmt::Arguments<'_>) {
        let mut sink = locked(self.sink(stream));
        if let Err(error) = sink.writer.write_fmt(text) {
            sink.failed(&error);
        }
    }

    /// Write `text` and a newline to `stream` under one lock, so a line is
    /// never split (what `outln!`/`errln!` expand to).
    pub(crate) fn write_line(&self, stream: Stream, text: fmt::Arguments<'_>) {
        let mut sink = locked(self.sink(stream));
        let written = sink
            .writer
            .write_fmt(text)
            .and_then(|()| sink.writer.write_all(b"\n"));
        if let Err(error) = written {
            sink.failed(&error);
        }
    }

    /// `document` as pretty JSON and a newline on stdout: every JSON
    /// document a core command answers with.
    pub(crate) fn json(&self, document: &impl Serialize) {
        let text = serde_json::to_string_pretty(document).expect("serialize JSON output");
        self.write_line(Stream::Out, format_args!("{text}"));
    }

    /// Each diagnostic as `severity[CODE]: message` on stderr, whatever the
    /// output format, so JSON stdout stays one document (registry
    /// configuration E067/W140/I003, fetch policy W155/W156, publish's
    /// declaration warnings).
    pub(crate) fn diagnostics(&self, diagnostics: &[Diagnostic]) {
        for diagnostic in diagnostics {
            self.write_line(
                Stream::Err,
                format_args!("{}", specforge_common::render_plain(diagnostic)),
            );
        }
    }

    /// The rest of the input (`specforge format --stdin`).
    pub(crate) fn read_to_string(&self) -> io::Result<String> {
        let mut text = String::new();
        locked(&self.0.input).read_to_string(&mut text)?;
        Ok(text)
    }

    /// Flush both streams; `main` calls it before it exits.
    pub(crate) fn flush(&self) {
        for stream in [Stream::Out, Stream::Err] {
            let mut sink = locked(self.sink(stream));
            if let Err(error) = sink.writer.flush() {
                sink.failed(&error);
            }
        }
    }
}

/// `println!` onto a terminal's stdout: `outln!(terminal, "…", args)`.
macro_rules! outln {
    ($t:expr) => { $t.write_line($crate::terminal::Stream::Out, format_args!("")) };
    ($t:expr, $($arg:tt)*) => {
        $t.write_line($crate::terminal::Stream::Out, format_args!($($arg)*))
    };
}

/// `print!` onto a terminal's stdout.
macro_rules! out {
    ($t:expr, $($arg:tt)*) => {
        $t.write($crate::terminal::Stream::Out, format_args!($($arg)*))
    };
}

/// `eprintln!` onto a terminal's stderr.
macro_rules! errln {
    ($t:expr) => { $t.write_line($crate::terminal::Stream::Err, format_args!("")) };
    ($t:expr, $($arg:tt)*) => {
        $t.write_line($crate::terminal::Stream::Err, format_args!($($arg)*))
    };
}

/// `eprint!` onto a terminal's stderr.
macro_rules! err {
    ($t:expr, $($arg:tt)*) => {
        $t.write($crate::terminal::Stream::Err, format_args!($($arg)*))
    };
}

pub(crate) use {err, errln, out, outln};

/// A buffer two sides share: a captured terminal writes it, a test reads it.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

#[cfg(test)]
impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        locked(&self.0).extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// What a captured terminal wrote.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Capture {
    out: SharedBuffer,
    err: SharedBuffer,
}

#[cfg(test)]
impl Capture {
    /// Everything written to stdout, as UTF-8.
    pub(crate) fn stdout(&self) -> String {
        String::from_utf8(locked(&self.out.0).clone()).expect("stdout is UTF-8")
    }

    /// Everything written to stderr, as UTF-8.
    pub(crate) fn stderr(&self) -> String {
        String::from_utf8(locked(&self.err.0).clone()).expect("stderr is UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::{Diagnostic, codes};
    use std::ffi::OsString;

    #[specforge_test_macros::test(
        behavior = "print_diagnostics_structured",
        verify = "NO_COLOR disables diagnostic colour"
    )]
    fn the_color_rule_is_no_color_then_force_then_the_terminal() {
        let value = |text: &str| Some(OsString::from(text));
        for (no_color, force, is_terminal, expected) in [
            // NO_COLOR set to anything non-empty wins over everything.
            (value("1"), value("1"), true, false),
            (value("1"), None, true, false),
            // An empty NO_COLOR is unset.
            (value(""), None, true, true),
            (value(""), None, false, false),
            // CLICOLOR_FORCE: anything non-empty other than "0".
            (None, value("1"), false, true),
            (None, value("0"), false, false),
            (None, value(""), false, false),
            (None, value("0"), true, true),
            // Otherwise the stream.
            (None, None, true, true),
            (None, None, false, false),
        ] {
            assert_eq!(
                color(no_color.as_deref(), force.as_deref(), is_terminal),
                expected,
                "NO_COLOR={no_color:?} CLICOLOR_FORCE={force:?} terminal={is_terminal}"
            );
        }
    }

    #[test]
    fn a_captured_terminal_keeps_each_stream() {
        let (terminal, capture) = Terminal::captured("", Facts::PIPED);

        outln!(terminal, "to stdout {}", 1);
        out!(terminal, "no newline");
        errln!(terminal, "to stderr");
        err!(terminal, "partial");
        terminal.json(&serde_json::json!({"a": 1}));
        terminal.diagnostics(&[Diagnostic::new(codes::W140, "careful")]);

        assert_eq!(
            capture.stdout(),
            "to stdout 1\nno newline{\n  \"a\": 1\n}\n"
        );
        assert!(
            capture.stderr().starts_with("to stderr\npartial"),
            "{}",
            capture.stderr()
        );
        assert!(
            capture.stderr().ends_with("careful\n"),
            "one warning line: {}",
            capture.stderr()
        );
        assert!(capture.stderr().contains("W140"), "{}", capture.stderr());
    }
}
