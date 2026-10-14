//! The MCP stdio transport (MCP 2025-11-25, "Transports: stdio") over any
//! pair of byte streams: what `specforge mcp` serves on its standard input
//! and output, and what a test drives in memory.

use std::io::{self, BufRead, Write};

use crate::McpServer;

/// Why a connection ended. In every case the client has been disconnected
/// ([`McpServer::disconnect`]): its subscriptions and listen streams ended.
#[derive(Debug)]
pub enum Ended {
    /// The input ended: the client closed its end.
    InputEnded,
    /// Reading the input failed.
    InputFailed(io::Error),
    /// Writing or flushing the output failed: nobody hears the server any
    /// more (`BrokenPipe` when the client stopped reading).
    OutputFailed(io::Error),
}

impl McpServer {
    /// Serve one client over `input` and `output` until the connection ends,
    /// then disconnect it.
    ///
    /// - Each line of `input` (ended by `\n`, `\r\n`, or the end of input)
    ///   is one message: a request, a notification or a batch. A line of
    ///   only JSON whitespace (space, tab, CR, LF) is skipped. A line that is
    ///   not UTF-8 is not JSON: it is answered with -32700 Parse error, and
    ///   serving goes on.
    /// - A message's response, when it has one, is written as one line, then
    ///   each notification the message queued, oldest first, one line each;
    ///   then `output` is flushed. Nothing else is written to `output`.
    /// - The connection ends at the end of `input`, at the first failed read,
    ///   or at the first failed write or flush.
    pub fn serve_stdio(&mut self, mut input: impl BufRead, mut output: impl Write) -> Ended {
        let ended = self.exchange(&mut input, &mut output);
        self.disconnect();
        ended
    }

    /// Read and answer messages until the connection ends.
    fn exchange(&mut self, input: &mut impl BufRead, output: &mut impl Write) -> Ended {
        let mut line = Vec::new();
        loop {
            line.clear();
            // `read_until` retries `Interrupted` itself; the bytes are never
            // decoded here, so a line that is not UTF-8 reaches the parser.
            match input.read_until(b'\n', &mut line) {
                Ok(0) => return Ended::InputEnded,
                Ok(_) => {}
                Err(error) => return Ended::InputFailed(error),
            }
            let message = line.trim_ascii();
            if message.is_empty() {
                continue;
            }
            if let Err(error) = self.answer(message, output) {
                return Ended::OutputFailed(error);
            }
        }
    }

    /// One message's reply: its response, then what it queued, then a flush.
    fn answer(&mut self, message: &[u8], output: &mut impl Write) -> io::Result<()> {
        if let Some(response) = self.handle_bytes(message) {
            output.write_all(response.as_bytes())?;
            output.write_all(b"\n")?;
        }
        for notification in self.take_notifications() {
            serde_json::to_writer(&mut *output, &notification)?;
            output.write_all(b"\n")?;
        }
        output.flush()
    }
}
