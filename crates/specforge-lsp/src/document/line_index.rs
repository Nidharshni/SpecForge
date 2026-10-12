//! Byte offsets and LSP positions (0-based line, UTF-16 column), both
//! ways: the one place the LSP converts them (ADR 0023).

use specforge_common::{SourceSpan, Sym};
use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range};

/// A character of a line that is wider in UTF-8 than in UTF-16 (any
/// non-ASCII character): its byte column, UTF-8 length and UTF-16 length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wide {
    col: u32,
    utf8: u8,
    utf16: u8,
}

/// Where each line of a text starts, and which of its characters are wider
/// in UTF-8 than in UTF-16: the one place the LSP converts byte offsets and
/// positions (0-based line, UTF-16 column), both ways (ADR 0023). A line
/// ends at `\n`; a `\r` before it belongs to the line, as clients count it.
/// Built in one pass; it owns no text, so it outlives the text it indexes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineIndex {
    /// Byte offset of each line's first byte.
    starts: Vec<u32>,
    /// Per line with any, its non-ASCII characters, in order.
    wide: HashMap<u32, Vec<Wide>>,
    /// Byte length of the text.
    len: u32,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0u32];
        let mut wide: HashMap<u32, Vec<Wide>> = HashMap::new();
        let mut line = 0u32;
        let mut line_start = 0usize;
        for (offset, ch) in text.char_indices() {
            if ch == '\n' {
                line += 1;
                line_start = offset + 1;
                starts.push(line_start as u32);
            } else if !ch.is_ascii() {
                wide.entry(line).or_default().push(Wide {
                    col: (offset - line_start) as u32,
                    utf8: ch.len_utf8() as u8,
                    utf16: ch.len_utf16() as u8,
                });
            }
        }
        LineIndex {
            starts,
            wide,
            len: text.len() as u32,
        }
    }

    /// The number of lines (a text ending in `\n` has an empty last line).
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The byte length of the text.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The byte offset of 0-based `line`'s first byte.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.starts.get(line).map(|s| *s as usize)
    }

    /// The byte offset just past 0-based `line`'s text, its `\n` excluded.
    pub fn line_end(&self, line: usize) -> Option<usize> {
        self.starts.get(line)?;
        Some(match self.starts.get(line + 1) {
            Some(next) => *next as usize - 1,
            None => self.len as usize,
        })
    }

    /// The non-ASCII characters of `line`.
    fn wides(&self, line: usize) -> &[Wide] {
        self.wide.get(&(line as u32)).map_or(&[], Vec::as_slice)
    }

    /// The byte column of UTF-16 column `character` on `line` (not
    /// clamped to the line). A column inside a character resolves to the
    /// character's start.
    fn byte_col(&self, line: usize, character: u32) -> usize {
        let (mut byte, mut units) = (0u32, 0u32);
        for wide in self.wides(line) {
            let ascii = wide.col - byte;
            if units + ascii >= character {
                return (byte + character - units) as usize;
            }
            units += ascii;
            byte = wide.col;
            if units + u32::from(wide.utf16) > character {
                return byte as usize;
            }
            units += u32::from(wide.utf16);
            byte += u32::from(wide.utf8);
        }
        (byte + character - units) as usize
    }

    /// The UTF-16 column of byte column `col` on `line`. A byte inside a
    /// character counts as the character's start.
    fn utf16_col(&self, line: usize, col: usize) -> u32 {
        let col = col as u32;
        let mut shift = 0u32;
        for wide in self.wides(line) {
            if wide.col >= col {
                break;
            }
            if wide.col + u32::from(wide.utf8) > col {
                // Inside the character: its start.
                return wide.col - shift;
            }
            shift += u32::from(wide.utf8) - u32::from(wide.utf16);
        }
        col - shift
    }

    /// The byte offset of `position`; `None` past the end of its line or of
    /// the text. A column inside a surrogate pair resolves to the start of
    /// its character.
    pub fn offset(&self, position: Position) -> Option<usize> {
        let line = position.line as usize;
        let start = self.line_start(line)?;
        let end = self.line_end(line)?;
        let offset = start + self.byte_col(line, position.character);
        (offset <= end).then_some(offset)
    }

    /// [`Self::offset`], clamped to its line's end and to the text's end:
    /// what an edit's range means.
    pub fn offset_clamped(&self, position: Position) -> usize {
        let line = position.line as usize;
        let (Some(start), Some(end)) = (self.line_start(line), self.line_end(line)) else {
            return self.len as usize;
        };
        (start + self.byte_col(line, position.character)).min(end)
    }

    /// The position of byte `offset`. An offset inside a character is that
    /// character's position; one past the end is the end.
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.len as usize) as u32;
        let line = match self.starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line - 1,
        };
        Position {
            line: line as u32,
            character: self.utf16_col(line, (offset - self.starts[line]) as usize),
        }
    }

    /// The position of 0-based `line` at byte column `col` (what rename
    /// edits name), clamped to the line and to the text.
    pub fn position_at(&self, line: usize, col: usize) -> Position {
        let line = line.min(self.starts.len() - 1);
        let start = self.starts[line] as usize;
        let end = self.line_end(line).unwrap_or(start);
        self.position((start + col).min(end))
    }

    /// The range of `span` (1-based lines, 1-based byte columns, end
    /// exclusive). A zero line or column counts as the first (an import
    /// target's span is all zeros); columns past a line's end clamp to it.
    pub fn range(&self, span: &SourceSpan) -> Range {
        let at = |line: usize, col: usize| {
            self.position_at(line.saturating_sub(1), col.saturating_sub(1))
        };
        Range {
            start: at(span.start_line, span.start_col),
            end: at(span.end_line, span.end_col),
        }
    }

    /// The span of `range` in `file`, its ends clamped to the text: what a
    /// code action request's range covers.
    pub fn span(&self, file: Sym, range: Range) -> SourceSpan {
        let at = |position: Position| {
            let offset = self.offset_clamped(position);
            let line = self.position(offset).line as usize;
            (line + 1, offset - self.starts[line] as usize + 1)
        };
        let (start_line, start_col) = at(range.start);
        let (end_line, end_col) = at(range.end);
        SourceSpan {
            file,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// The 1-based (line, byte column) of `position`, as navigation counts
    /// positions; `None` past the end of its line or of the text.
    pub fn source_position(&self, position: Position) -> Option<(usize, usize)> {
        let offset = self.offset(position)?;
        let line = position.line as usize;
        Some((line + 1, offset - self.starts[line] as usize + 1))
    }
}
