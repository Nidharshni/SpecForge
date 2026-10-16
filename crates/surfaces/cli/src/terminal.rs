//! Where a core command writes and whom it asks (ADR 0029 D4b).
//!
//! `main` makes the process's terminal ([`Terminal::process`]) and hands it to
//! the command it runs; a test captures one (`Terminal::captured`). No other
//! module names the process's stdio: the crate denies `clippy::print_stdout`
//! and `clippy::print_stderr`, and only this module calls `std::io::stdout`,
//! `stderr`, `stdin` or `IsTerminal`.

use serde::Serialize;
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

/// One output stream: its writer, the name std's panic gives it, and
/// whether its reader has gone.
struct Sink {
    writer: Box<dyn Write + Send>,
    label: &'static str,
    closed: bool,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Sink {
    fn new(writer: impl Write + Send + 'static, label: &'static str) -> Mutex<Self> {
        Mutex::new(Self {
            writer: Box::new(writer),
            label,
            closed: false,
        })
    }

    /// Run one write and report a reader that has gone: once it has gone,
    /// nothing is written and the result is `BrokenPipe`; a reader that goes
    /// now (`BrokenPipe`) closes the stream and is reported. Any other
    /// failure panics as `print!` does: a full disk is not a reader that
    /// left.
    fn reporting(
        &mut self,
        write: impl FnOnce(&mut (dyn Write + Send)) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        match write(&mut *self.writer) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                self.closed = true;
                Err(error)
            }
            Err(error) => self.failed(&error),
        }
    }

    /// Run one write; a reader that has gone is not an error (`out!`,
    /// `outln!`, …).
    fn emit(&mut self, write: impl FnOnce(&mut (dyn Write + Send)) -> io::Result<()>) {
        let _ = self.reporting(write);
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
    /// reader that has gone (`BrokenPipe`) closes the stream, and later
    /// writes to it are dropped; any other failed write panics as `print!`
    /// does: `failed printing to stdout: …`.
    pub(crate) fn write(&self, stream: Stream, text: fmt::Arguments<'_>) {
        locked(self.sink(stream)).emit(|writer| writer.write_fmt(text));
    }

    /// Write `text` and a newline to `stream` under one lock, so a line is
    /// never split (what `outln!`/`errln!` expand to).
    pub(crate) fn write_line(&self, stream: Stream, text: fmt::Arguments<'_>) {
        locked(self.sink(stream)).emit(|writer| {
            writer.write_fmt(text)?;
            writer.write_all(b"\n")
        });
    }

    /// Whether stdout's reader has gone: a long-running command (`watch`,
    /// `mcp`) stops when it has.
    pub(crate) fn closed(&self) -> bool {
        locked(&self.0.out).closed
    }

    /// `document` as pretty JSON and a newline on stdout: every JSON
    /// document a core command answers with.
    pub(crate) fn json(&self, document: &impl Serialize) {
        let text = serde_json::to_string_pretty(document).expect("serialize JSON output");
        self.write_line(Stream::Out, format_args!("{text}"));
    }

    /// Ask `question` (written to stderr as it is, then flushed) and read one
    /// line: yes when the trimmed answer is `y` or `yes` in any case, no
    /// otherwise, on end of input or a read error. With nobody to ask
    /// (`!facts().interactive`) it is no, and nothing is written or read:
    /// blocking on a pipe nobody answers would hang.
    pub(crate) fn ask(&self, question: &str) -> bool {
        if !self.facts().interactive {
            return false;
        }
        self.write(Stream::Err, format_args!("{question}"));
        {
            let mut err = locked(&self.0.err);
            // A question must reach the person before they answer it.
            let _ = err.writer.flush();
        }
        let mut answer = String::new();
        match locked(&self.0.input).read_line(&mut answer) {
            Ok(0) | Err(_) => false,
            Ok(_) => matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
        }
    }

    /// The whole input, held until the value drops: for a command that
    /// serves its input to the end and asks nothing (`specforge mcp`). The
    /// bytes are handed on as they are, never decoded. Asking (`ask`) while
    /// it is held would wait for it.
    pub(crate) fn input(&self) -> Input<'_> {
        Input(locked(&self.0.input))
    }

    /// Stdout as a writer that reports what [`Self::write`] hides: a write
    /// once the reader has gone, or one that finds it gone (`BrokenPipe`,
    /// which closes the stream), fails with `BrokenPipe`; any other failure
    /// panics as `print!` does (`specforge mcp`, whose transport stops at
    /// the first failed write).
    pub(crate) fn output(&self) -> Output<'_> {
        Output(self)
    }

    /// A writer onto `stream` for code that wants `&mut dyn Write`
    /// (`clap_complete::generate`). It locks the stream per write call,
    /// never across calls.
    pub(crate) fn writer(&self, stream: Stream) -> StreamWriter<'_> {
        StreamWriter {
            terminal: self,
            stream,
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
            locked(self.sink(stream)).emit(|writer| writer.flush());
        }
    }
}

/// What [`Terminal::input`] gives.
pub(crate) struct Input<'t>(MutexGuard<'t, Box<dyn BufRead + Send>>);

impl Read for Input<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl BufRead for Input<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.0.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.0.consume(amount)
    }
}

/// What [`Terminal::output`] gives.
pub(crate) struct Output<'t>(&'t Terminal);

impl Write for Output<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        locked(self.0.sink(Stream::Out)).reporting(|writer| writer.write_all(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        locked(self.0.sink(Stream::Out)).reporting(|writer| writer.flush())
    }
}

/// What [`Terminal::writer`] gives.
pub(crate) struct StreamWriter<'t> {
    terminal: &'t Terminal,
    stream: Stream,
}

impl Write for StreamWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        locked(self.terminal.sink(self.stream)).emit(|writer| writer.write_all(buf));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        locked(self.terminal.sink(self.stream)).emit(|writer| writer.flush());
        Ok(())
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

    const INTERACTIVE: Facts = Facts {
        color_out: false,
        color_err: false,
        interactive: true,
    };

    #[test]
    fn ask_takes_y_or_yes_in_any_case_and_nothing_else() {
        for (input, answer) in [
            ("y\n", true),
            ("yes\n", true),
            (" YES \n", true),
            ("Yes", true),
            ("n\n", false),
            ("\n", false),
            ("", false),
            ("yess\n", false),
        ] {
            let (terminal, capture) = Terminal::captured(input, INTERACTIVE);
            assert_eq!(terminal.ask("Go? "), answer, "{input:?}");
            assert_eq!(capture.stderr(), "Go? ", "the question, once: {input:?}");
        }
    }

    #[test]
    fn nobody_to_ask_is_no_and_nothing_is_written() {
        let (terminal, capture) = Terminal::captured("y\n", Facts::PIPED);
        assert!(!terminal.ask("Go? "));
        assert_eq!(capture.stderr(), "");
        assert_eq!(terminal.read_to_string().unwrap(), "y\n", "input untouched");
    }

    #[test]
    fn output_reports_a_reader_that_has_gone() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let terminal = Terminal::new(
            BrokenPipeWriter(attempts.clone()),
            io::sink(),
            io::empty(),
            Facts::PIPED,
        );
        let mut output = terminal.output();

        let error = output.write(b"one").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(terminal.closed());
        let error = output.write(b"two").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(
            attempts.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "nothing is written to a stream whose reader has gone"
        );
    }

    #[test]
    fn input_hands_the_bytes_on_as_they_are() {
        let terminal = Terminal::new(
            io::sink(),
            io::sink(),
            io::Cursor::new(b"\xff\xfe\nok\n".to_vec()),
            Facts::PIPED,
        );
        let mut input = terminal.input();
        let mut line = Vec::new();
        input.read_until(b'\n', &mut line).unwrap();
        assert_eq!(line, b"\xff\xfe\n");
        line.clear();
        input.read_until(b'\n', &mut line).unwrap();
        assert_eq!(line, b"ok\n");
    }

    /// A reader that has gone: every write is a broken pipe.
    struct BrokenPipeWriter(Arc<std::sync::atomic::AtomicUsize>);

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "a closed stdout ends the output quietly and keeps the command's exit"
    )]
    fn a_closed_stdout_ends_the_output_quietly() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let err = SharedBuffer::default();
        let terminal = Terminal::new(
            BrokenPipeWriter(attempts.clone()),
            err.clone(),
            io::empty(),
            Facts::PIPED,
        );
        assert!(!terminal.closed());

        outln!(terminal, "one");
        assert!(terminal.closed());
        outln!(terminal, "two");
        terminal.json(&serde_json::json!({"three": 3}));
        terminal.flush();

        assert_eq!(
            attempts.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "nothing is written to a stream whose reader has gone"
        );
        errln!(terminal, "still here");
        assert_eq!(
            String::from_utf8(locked(&err.0).clone()).unwrap(),
            "still here\n"
        );
    }

    #[test]
    fn a_captured_terminal_keeps_each_stream() {
        let (terminal, capture) = Terminal::captured("", Facts::PIPED);

        outln!(terminal, "to stdout {}", 1);
        out!(terminal, "no newline");
        errln!(terminal, "to stderr");
        err!(terminal, "partial");
        terminal.json(&serde_json::json!({"a": 1}));

        assert_eq!(
            capture.stdout(),
            "to stdout 1\nno newline{\n  \"a\": 1\n}\n"
        );
        assert!(
            capture.stderr().starts_with("to stderr\npartial"),
            "{}",
            capture.stderr()
        );
    }
}
