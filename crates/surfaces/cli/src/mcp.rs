use crate::outcome::Exit;
use crate::terminal::Terminal;
use specforge_mcp::McpServer;
use std::path::PathBuf;

// `specforge mcp`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Where to find the project to serve: the nearest directory at or above it holding
    /// specforge.json (none yet: served once one exists there; in a handshake session a call
    /// may also serve the project its path names or init creates)
    #[arg(default_value = ".")]
    root: PathBuf,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        // The client drives the handshake; its `initialize` serves the
        // project at or above `root` unless it names a `projectRoot` of its
        // own; a stateless request is served it without one. However the connection ends, the client has left and the
        // command passed: a closed stdout ends the output quietly, as for
        // every command (ADR 0029 D4b), and a broken input is the client's
        // end (ADR 0062 D4).
        McpServer::with_project_root(self.root).serve_stdio(terminal.input(), terminal.output());
        Exit::Passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Facts;
    use std::io::{self, BufReader, Read, Write};

    /// A reader of stdout that has gone: every write is a broken pipe.
    struct Gone;

    impl Write for Gone {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    /// An input that never ends: one ping line, forever.
    struct Endless {
        at: usize,
    }

    const PING: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(PING.len() - self.at);
            buf[..n].copy_from_slice(&PING[self.at..self.at + n]);
            self.at = (self.at + n) % PING.len();
            Ok(n)
        }
    }

    #[specforge_test_macros::test(
        behavior = "report_command_outcome",
        verify = "a closed stdout ends the output quietly and keeps the command's exit"
    )]
    fn a_closed_stdout_ends_the_server_while_its_input_continues() {
        let dir = tempfile::tempdir().unwrap();
        let terminal = Terminal::new(
            Gone,
            io::sink(),
            BufReader::new(Endless { at: 0 }),
            Facts::PIPED,
        );

        let exit = Args {
            root: dir.path().to_path_buf(),
        }
        .run(&terminal);

        assert!(matches!(exit, Exit::Passed));
        assert!(terminal.closed());
    }
}
