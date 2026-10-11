use crate::outcome::Exit;
use specforge_mcp::McpServer;
use std::io::{self, BufRead, Write};
use std::path::Path;

pub fn run(path: &Path) -> Exit {
    // The client drives the handshake; its `initialize` serves the project at
    // or above `path` unless it names a `projectRoot` of its own.
    let mut server = McpServer::with_project_root(path.to_path_buf());

    // Stdio loop: read JSON-RPC from stdin, write responses to stdout
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = server.handle_message(trimmed);
        let mut out = stdout.lock();
        if let Some(response) = response {
            let _ = writeln!(out, "{}", response);
        }
        // What this request queued for its subscriptions follows its response.
        for notification in server.take_notifications() {
            let _ = writeln!(out, "{}", notification);
        }
        let _ = out.flush();
    }

    // End of input: the stdio client is gone.
    server.disconnect();
    Exit::Passed
}
