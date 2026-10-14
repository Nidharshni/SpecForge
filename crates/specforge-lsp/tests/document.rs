use specforge_common::{SourceSpan, Sym};
use specforge_lsp::{Document, LineIndex};
use specforge_test_macros::test as spec;
use tower_lsp::lsp_types::Position;

// -- incremental_document_sync ------------------------------------------------

#[spec(
    behavior = "incremental_document_sync",
    verify = "incremental change applies correctly to source buffer"
)]
fn incremental_change_applies_correctly() {
    let mut buf = Document::new("behavior foo \"Foo\" {\n  contract \"old\"\n}\n".into());

    // Replace "old" with "new" (line 1, col 12..15)
    buf.apply_change(Some(crate::lsp_range(1, 12, 1, 15)), "new");

    assert_eq!(
        buf.text(),
        "behavior foo \"Foo\" {\n  contract \"new\"\n}\n"
    );
}

#[spec(
    behavior = "incremental_document_sync",
    verify = "multiple incremental changes produce correct source"
)]
fn multiple_incremental_changes_produce_correct_source() {
    let mut buf = Document::new("line0\nline1\nline2\n".into());

    // Replace "line1" with "REPLACED"
    buf.apply_change(Some(crate::lsp_range(1, 0, 1, 5)), "REPLACED");
    assert_eq!(buf.text(), "line0\nREPLACED\nline2\n");

    // Insert at start of line2
    buf.apply_change(Some(crate::lsp_range(2, 0, 2, 0)), "prefix_");
    assert_eq!(buf.text(), "line0\nREPLACED\nprefix_line2\n");
}

#[spec(
    behavior = "incremental_document_sync",
    verify = "incremental sync reduces transfer size vs full sync"
)]
#[tokio::test]
async fn incremental_sync_reduces_transfer_size() {
    use crate::session::Session;
    use serde_json::json;

    let (mut session, init) = Session::start(None).await;
    // The server asks for INCREMENTAL sync: changes, not whole documents.
    assert_eq!(init["capabilities"]["textDocumentSync"], 2);

    let uri = "file:///buffer/sync.spec";
    let padding = "  // a long comment the edit never touches\n".repeat(40);
    let text = format!(
        "behavior login \"Login\" {{\n{padding}  invariants [session_limit]\n}}\n\n\
         invariant session_limit \"Limit\" {{\n}}\n"
    );
    session.open(uri, &text).await;
    assert!(session.diagnostics(uri).await.is_empty());

    // Rename the invariant: only the changed range travels.
    let line = text
        .lines()
        .position(|l| l.starts_with("invariant"))
        .unwrap() as u32;
    let change = json!({"range": {
        "start": {"line": line, "character": 10},
        "end": {"line": line, "character": 23},
    }, "text": "quota"});
    let full = json!({"text": text.replacen("invariant session_limit", "invariant quota", 1)});
    let sent = change.to_string().len();
    assert!(
        sent * 10 < full.to_string().len(),
        "{sent} bytes vs {} for full sync",
        full.to_string().len()
    );
    session
        .notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [change]}),
        )
        .await;

    // The server rebuilt the whole document from the range: the reference
    // is now dangling and the outline names the renamed invariant.
    let diagnostics = session.diagnostics(uri).await;
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d["message"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["unresolved reference 'session_limit' in entity 'login'"]
    );
    let outline = session
        .request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": uri}}),
        )
        .await;
    let names = outline["result"].to_string();
    assert!(
        names.contains("quota") && !names.contains("session_limit"),
        "{names}"
    );
}

// -- utf16_positions -----------------------------------------------------------

#[spec(
    behavior = "incremental_document_sync",
    verify = "incremental change applies correctly to source buffer"
)]
fn utf16_columns_resolve_to_byte_offsets_after_multibyte_chars() {
    // Line 0 layout: `behavior foo "🚀世" {`
    //   `behavior foo "` = 14 UTF-16 units / 14 bytes
    //   🚀 = 2 UTF-16 units / 4 bytes (units 14..16, bytes 14..18)
    //   世 = 1 UTF-16 unit / 3 bytes (unit 16..17, bytes 18..21)
    //   `"` at unit 17..18 (byte 21..22), `{` at unit 19 (byte 23)
    let mut buf = Document::new("behavior foo \"🚀世\" {\n}\n".into());

    // Replace the emoji via UTF-16 columns 14..16.
    buf.apply_change(Some(crate::lsp_range(0, 14, 0, 16)), "🌟");
    assert_eq!(buf.text(), "behavior foo \"🌟世\" {\n}\n");

    // Replace the CJK char via UTF-16 column 16..17 (byte offset must land
    // after the 4-byte emoji, i.e. at byte 18).
    buf.apply_change(Some(crate::lsp_range(0, 16, 0, 17)), "界");
    assert_eq!(buf.text(), "behavior foo \"🌟界\" {\n}\n");

    // Column after all multibyte chars: insert at unit 18 = byte 22 (right
    // after the closing quote).
    buf.apply_change(Some(crate::lsp_range(0, 18, 0, 18)), " // 🌟");
    assert_eq!(buf.text(), "behavior foo \"🌟界\" // 🌟 {\n}\n");
}

// -- line_index -----------------------------------------------------------------

/// Every `.spec` text under `dir`, recursively.
fn spec_texts(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            spec_texts(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(std::fs::read_to_string(&path).unwrap());
        }
    }
}

/// The oracle: the position of every char boundary of `text` (and of its
/// end), by walking its characters and summing their UTF-16 lengths.
fn walked_positions(text: &str) -> Vec<(usize, Position)> {
    let mut positions = Vec::new();
    let (mut line, mut character) = (0, 0);
    for (at, ch) in text.char_indices() {
        positions.push((at, Position::new(line, character)));
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    positions.push((text.len(), Position::new(line, character)));
    positions
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "the line index converts byte columns to UTF-16 and back on every line"
)]
fn line_index_agrees_with_a_char_walk_on_every_line() {
    let spec = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec");
    let mut texts = Vec::new();
    spec_texts(&spec, &mut texts);
    assert!(texts.len() > 150, "found only {} spec files", texts.len());
    texts.push("behavior é \"世🚀\" {\r\n  contract \"a🚀b\"\r\n}\r\n\nlast é".to_string());
    for text in &texts {
        let index = LineIndex::new(text);
        for (offset, position) in walked_positions(text) {
            assert_eq!(index.position(offset), position, "offset {offset}");
            assert_eq!(index.offset(position), Some(offset), "{position:?}");
            assert_eq!(index.offset(index.position(offset)), Some(offset));
        }
    }
}

#[spec(
    invariant = "lsp_utf16_positions",
    verify = "the line index converts byte columns to UTF-16 and back on every line"
)]
fn positions_past_a_line_end() {
    // `a🚀b`: 🚀 is 4 bytes and 2 UTF-16 units (columns 1..3).
    let text = "a🚀b\nxy";
    let index = LineIndex::new(text);
    assert_eq!(index.offset(Position::new(0, 5)), None, "past the line");
    assert_eq!(index.offset(Position::new(2, 0)), None, "past the text");
    assert_eq!(
        index.offset_clamped(Position::new(0, 9)),
        6,
        "the line's end"
    );
    assert_eq!(index.offset_clamped(Position::new(7, 0)), text.len());
    assert_eq!(
        index.offset(Position::new(0, 2)),
        Some(1),
        "a surrogate pair's middle is its character's start"
    );
    assert_eq!(index.position(3), Position::new(0, 1), "inside a character");
    assert_eq!(index.position_at(0, 99), Position::new(0, 4), "clamped");
    let zero = SourceSpan {
        file: Sym::new("f"),
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 0,
    };
    let range = index.range(&zero);
    assert_eq!(
        (range.start, range.end),
        (Position::new(0, 0), Position::new(0, 0))
    );
    let span = index.span(Sym::new("f"), crate::lsp_range(0, 3, 1, 9));
    assert_eq!(
        (span.start_line, span.start_col, span.end_line, span.end_col),
        (1, 6, 2, 3),
        "a range's ends clamp to the text"
    );
}

// -- a column no line has --------------------------------------------------------

#[test]
#[ignore = "fixed by T2"]
fn a_column_past_the_line_end_resolves_to_the_line_end() {
    for text in ["éé\n", "// éé\n", "a😀b\n", "ascii\n"] {
        let index = LineIndex::new(text);
        let end = index.line_end(0).unwrap();
        for character in [end as u32 + 1, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            let at = Position { line: 0, character };
            assert_eq!(index.offset(at), None, "{text:?} at {character}");
            assert_eq!(index.offset_clamped(at), end, "{text:?} at {character}");
        }
    }
}

#[test]
#[ignore = "fixed by T2"]
fn an_edit_at_the_largest_column_applies_at_the_line_end() {
    for (text, expected) in [("éé\n", "ééx\n"), ("// éé\n", "// ééx\n")] {
        let mut doc = Document::new(text.into());
        doc.apply_change(Some(crate::lsp_range(0, u32::MAX, 0, u32::MAX)), "x");
        assert_eq!(doc.text(), expected);
    }
}
