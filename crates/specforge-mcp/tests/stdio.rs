//! The MCP stdio transport (`McpServer::serve_stdio`), driven in memory:
//! the rules `specforge mcp` serves on its standard input and output.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_mcp::stdio::Ended;
use specforge_test::prelude::*;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};

/// One JSON-RPC request as a line (no line break).
fn line(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

/// `initialize` for protocol 2025-03-26, which accepts batches.
fn initialize(id: u64) -> String {
    line(
        id,
        "initialize",
        json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "stdio", "version": "0"},
        }),
    )
}

/// `served()`'s server over `input`: how it ended and every line it wrote,
/// parsed. Every line of output ends in `\n` and is JSON.
fn exchange(server: &mut McpServer, input: &[u8]) -> (Ended, Vec<Value>) {
    let mut output = Vec::new();
    let ended = server.serve_stdio(Cursor::new(input.to_vec()), &mut output);
    (ended, parse_lines(&output))
}

/// The lines of `output`, each ended by `\n` and a JSON document.
fn parse_lines(output: &[u8]) -> Vec<Value> {
    if output.is_empty() {
        return Vec::new();
    }
    assert_eq!(output.last(), Some(&b'\n'), "the last line is not ended");
    String::from_utf8(output.to_vec())
        .expect("output is UTF-8")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
        .collect()
}

/// An initialized server over one entity, subscribed to nothing.
fn served() -> Served {
    TestProject::new()
        .file("spec/base.spec", "behavior base \"Base\" {\n}\n")
        .serve(&[TestExtension::software()])
}

/// What a [`Recorder`] saw, in order.
#[derive(Debug)]
enum Did {
    Wrote(Vec<u8>),
    Flushed,
}

/// A writer that records every write and flush.
#[derive(Default)]
struct Recorder(Vec<Did>);

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.push(Did::Wrote(buf.to_vec()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.push(Did::Flushed);
        Ok(())
    }
}

impl Recorder {
    /// The bytes written between flushes, one entry per flush; panics when
    /// something was written after the last flush.
    fn flushed_parts(&self) -> Vec<Vec<u8>> {
        let mut parts = Vec::new();
        let mut pending = Vec::new();
        for did in &self.0 {
            match did {
                Did::Wrote(bytes) => pending.extend_from_slice(bytes),
                Did::Flushed => parts.push(std::mem::take(&mut pending)),
            }
        }
        assert!(pending.is_empty(), "written after the last flush");
        parts
    }
}

/// A writer whose reader has gone.
struct Gone;

impl Write for Gone {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}

/// A reader that fails.
struct Broken;

impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("the pipe broke"))
    }
}

/// What a client does before it sends a line.
type Act = Box<dyn FnOnce()>;

/// An input of lines, each preceded by an action run just before the line
/// is first read: a client that acts between two of its messages.
struct Script {
    lines: Vec<(Option<Act>, Vec<u8>)>,
    at: usize,
    offset: usize,
}

impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for Script {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.at >= self.lines.len() {
            return Ok(&[]);
        }
        if self.offset == 0
            && let Some(act) = self.lines[self.at].0.take()
        {
            act();
        }
        Ok(&self.lines[self.at].1[self.offset..])
    }

    fn consume(&mut self, amount: usize) {
        self.offset += amount;
        if self.at < self.lines.len() && self.offset >= self.lines[self.at].1.len() {
            self.at += 1;
            self.offset = 0;
        }
    }
}

fn ping(id: u64) -> String {
    line(id, "ping", json!({}))
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "each line of input is one message, and each response or notification is one line of output"
)]
fn each_line_is_one_message_and_each_reply_one_line() {
    let dir = TestProject::new()
        .file("spec/base.spec", "behavior base \"Base\" {\n}\n")
        .into_dir();
    let mut server = McpServer::with_project_root(dir.path().to_path_buf());
    let stateless_listen = line(
        4,
        "subscriptions/listen",
        json!({
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
            },
            "notifications": {"resourceSubscriptions": ["specforge://graph"]},
        }),
    );
    let input = [
        initialize(1),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        format!("[{},{}]", ping(2), ping(3)),
        stateless_listen,
        // The last line has no line break.
        ping(5),
    ]
    .join("\n");

    let (ended, lines) = exchange(&mut server, input.as_bytes());

    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0]["id"], 1, "initialize: {}", lines[0]);
    let batch = lines[1]
        .as_array()
        .expect("a batch is answered by an array");
    assert_eq!(batch.len(), 2, "{batch:?}");
    assert_eq!(batch[0]["id"], 2);
    assert_eq!(batch[1]["id"], 3);
    assert_eq!(
        lines[2]["method"], "notifications/subscriptions/acknowledged",
        "{}",
        lines[2]
    );
    assert_eq!(lines[3]["id"], 5, "{}", lines[3]);
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "a message's response is written before the notifications it queued"
)]
fn a_response_precedes_the_notifications_it_queued() {
    let mut server = served();
    let subscribed = call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(subscribed["result"].is_object(), "{subscribed}");
    server.write("spec/added.spec", "behavior added \"Added\" {\n}\n");

    let read = line(
        2,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );
    let (ended, lines) = exchange(&mut server, format!("{read}\n").as_bytes());

    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0]["id"], 2, "the response comes first: {}", lines[0]);
    assert_eq!(lines[1]["method"], "notifications/resources/updated");
    assert_eq!(lines[1]["params"]["uri"], "specforge://graph");
    assert_eq!(lines[2]["method"], "specforge/graphChanged");
    assert!(lines[2].get("id").is_none(), "{}", lines[2]);
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "the output is flushed after each message"
)]
fn the_output_is_flushed_after_each_message() {
    let mut server = served();
    let input = format!("{}\n\n  \n{}\n", ping(1), ping(2));
    let mut recorder = Recorder::default();

    let ended = server.serve_stdio(Cursor::new(input.into_bytes()), &mut recorder);

    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    let parts = recorder.flushed_parts();
    assert_eq!(parts.len(), 2, "one flush per message, none per blank line");
    for (part, id) in parts.iter().zip([1, 2]) {
        assert_eq!(part.last(), Some(&b'\n'));
        assert_eq!(part.iter().filter(|b| **b == b'\n').count(), 1);
        let reply: Value = serde_json::from_slice(part).unwrap();
        assert_eq!(reply["id"], id, "{reply}");
    }
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "a line holding only whitespace is skipped"
)]
fn a_line_of_only_whitespace_is_skipped() {
    let mut server = served();
    let input = format!("\n   \n\t\r\n{}\r\n  {}  \n", ping(1), ping(2));

    let (ended, lines) = exchange(&mut server, input.as_bytes());

    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    let ids: Vec<&Value> = lines.iter().map(|l| &l["id"]).collect();
    assert_eq!(ids, [1, 2], "{lines:?}");
    assert!(lines.iter().all(|l| l.get("error").is_none()), "{lines:?}");
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "a line that is not UTF-8 is answered with -32700 Parse error and serving continues"
)]
fn a_line_that_is_not_utf8_is_a_parse_error_and_serving_continues() {
    let mut server = served();
    let mut input = b"\xff\xfe\n".to_vec();
    input.extend_from_slice(format!("{}\n", ping(2)).as_bytes());
    // Latin-1 inside a JSON string.
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{"x":"caf"#);
    input.push(0xE9);
    input.extend_from_slice(b"\"}}\n");
    input.extend_from_slice(format!("{}\n", ping(4)).as_bytes());

    let (ended, lines) = exchange(&mut server, &input);

    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    assert_eq!(lines.len(), 4, "{lines:?}");
    for bad in [&lines[0], &lines[2]] {
        assert!(bad["id"].is_null(), "{bad}");
        assert_eq!(bad["error"]["code"], -32700, "{bad}");
        assert_eq!(bad["error"]["message"], "Parse error", "{bad}");
    }
    assert_eq!(lines[1]["id"], 2);
    assert_eq!(lines[3]["id"], 4);
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "the end of the input disconnects the client"
)]
fn the_end_of_the_input_disconnects_the_client() {
    let mut server = served();
    let subscribed = call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(subscribed["result"].is_object(), "{subscribed}");
    listen(&mut server, json!(7), &["specforge://diagnostics"]);
    assert!(!server.state().subscriptions().is_empty());

    let (ended, lines) = exchange(&mut server, b"");

    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    assert!(lines.is_empty(), "{lines:?}");
    assert!(server.state().subscriptions().is_empty());
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "a failed write ends the connection and disconnects the client"
)]
fn a_failed_write_ends_the_connection_and_disconnects_the_client() {
    let mut server = served();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(!server.state().subscriptions().is_empty());
    let first = format!("{}\n", ping(1));
    let mut input = Cursor::new(format!("{first}{}\n", ping(2)).into_bytes());

    let ended = server.serve_stdio(&mut input, Gone);

    match ended {
        Ended::OutputFailed(error) => assert_eq!(error.kind(), io::ErrorKind::BrokenPipe),
        other => panic!("expected OutputFailed, got {other:?}"),
    }
    assert_eq!(
        input.position(),
        first.len() as u64,
        "nothing is read after the failure"
    );
    assert!(server.state().subscriptions().is_empty());
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "a failed read ends the connection and disconnects the client"
)]
fn a_failed_read_ends_the_connection_and_disconnects_the_client() {
    let mut server = served();
    call(
        &mut server,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    assert!(!server.state().subscriptions().is_empty());

    let ended = server.serve_stdio(BufReader::new(Broken), Vec::new());

    match ended {
        Ended::InputFailed(error) => assert_eq!(error.to_string(), "the pipe broke"),
        other => panic!("expected InputFailed, got {other:?}"),
    }
    assert!(server.state().subscriptions().is_empty());
}

#[specforge_test(
    behavior = "serve_mcp_over_stdio",
    verify = "Serve MCP over Stdio: stdio transport holds — connection_open, one_line_per_message, response_first, connection_ended"
)]
fn stdio_transport_contract() {
    // connection_open: an initialized server, its input and output connected.
    let mut server = served();
    let root = server.root().to_path_buf();
    let subscribe = line(
        1,
        "resources/subscribe",
        json!({"uri": "specforge://graph"}),
    );
    let read = line(
        2,
        "resources/read",
        json!({"uri": "specforge://diagnostics"}),
    );
    let script = Script {
        lines: vec![
            (None, format!("{subscribe}\n").into_bytes()),
            // The client edits a file between two of its messages.
            (
                Some(Box::new(move || {
                    std::fs::write(
                        root.join("spec/added.spec"),
                        "behavior added \"Added\" {\n}\n",
                    )
                    .unwrap();
                })),
                format!("{read}\n").into_bytes(),
            ),
            (None, format!("{}\n", ping(3)).into_bytes()),
        ],
        at: 0,
        offset: 0,
    };
    let mut recorder = Recorder::default();

    let ended = server.serve_stdio(script, &mut recorder);

    // connection_ended: the input ended, and the client is disconnected.
    assert!(matches!(ended, Ended::InputEnded), "{ended:?}");
    assert!(server.state().subscriptions().is_empty());
    // one_line_per_message, response_first: one flush per message, each
    // holding its response, then the notifications it queued.
    let parts: Vec<Vec<Value>> = recorder
        .flushed_parts()
        .iter()
        .map(|part| parse_lines(part))
        .collect();
    assert_eq!(parts.len(), 3, "{parts:?}");
    assert_eq!(parts[0].len(), 1);
    assert_eq!(parts[0][0]["id"], 1);
    assert_eq!(parts[1].len(), 3, "{:?}", parts[1]);
    assert_eq!(parts[1][0]["id"], 2);
    assert_eq!(parts[1][1]["method"], "notifications/resources/updated");
    assert_eq!(parts[1][2]["method"], "specforge/graphChanged");
    assert_eq!(parts[2].len(), 1);
    assert_eq!(parts[2][0]["id"], 3);
}
