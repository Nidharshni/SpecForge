#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod answers;
mod changes;
mod code_actions;
mod completion;
mod concurrency;
mod contracts;
mod cursor;
mod document;
mod e2e;
mod hover;
mod identity;
mod lifecycle;
mod navigation;
mod protocol;
mod publish;
mod published;
mod reaction;
mod recorder;
mod registries;
mod rename;
mod semantic_tokens;
mod served;
mod session;
mod state;

/// Every `.spec` file under `dir`, recursively.
pub fn spec_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            spec_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
}

/// The URI `text` names.
pub fn url(text: &str) -> tower_lsp::lsp_types::Url {
    tower_lsp::lsp_types::Url::parse(text).unwrap()
}

/// A content change replacing `range` (the whole text when `None`) with `text`.
pub fn change(
    range: Option<tower_lsp::lsp_types::Range>,
    text: &str,
) -> tower_lsp::lsp_types::TextDocumentContentChangeEvent {
    tower_lsp::lsp_types::TextDocumentContentChangeEvent {
        range,
        range_length: None,
        text: text.to_string(),
    }
}

/// A content change replacing the whole text with `text`.
pub fn whole(text: &str) -> tower_lsp::lsp_types::TextDocumentContentChangeEvent {
    change(None, text)
}

/// The LSP range from (`start_line`, `start_character`) to (`end_line`,
/// `end_character`): 0-based lines, UTF-16 columns.
pub fn lsp_range(
    start_line: u32,
    start_character: u32,
    end_line: u32,
    end_character: u32,
) -> tower_lsp::lsp_types::Range {
    tower_lsp::lsp_types::Range::new(
        tower_lsp::lsp_types::Position::new(start_line, start_character),
        tower_lsp::lsp_types::Position::new(end_line, end_character),
    )
}
