use crate::outcome::Exit;
use crate::terminal::{Terminal, outln};
use specforge_mcp::McpServer;
use std::path::PathBuf;

// `specforge mcp`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Where to find the project to serve: the nearest directory at or above it holding
    /// specforge.json or specforge.spec (none: nothing is served until a call names a
    /// project or init creates one)
    #[arg(default_value = ".")]
    path: PathBuf,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        // The client drives the handshake; its `initialize` serves the
        // project at or above `path` unless it names a `projectRoot` of its
        // own.
        let mut server = McpServer::with_project_root(self.path);

        // Stdio loop: read JSON-RPC from the terminal's input, write
        // responses to its output.
        while let Some(line) = terminal.read_line() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(response) = server.handle_message(trimmed) {
                outln!(terminal, "{}", response);
            }
            // What this request queued for its subscriptions follows its
            // response.
            for notification in server.take_notifications() {
                outln!(terminal, "{}", notification);
            }
            terminal.flush();
        }

        // End of input: the stdio client is gone.
        server.disconnect();
        Exit::Passed
    }
}
