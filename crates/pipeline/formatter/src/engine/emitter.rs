use super::block::format_block;
use super::{
    FormatResult, collect_error_regions, comment_text, holds_comment, kept_region, one_line,
    overlaps,
};
use crate::config::FormatConfig;
use specforge_parser::cst::Piece;
use tree_sitter::Node;
use tree_sitter_specforge::kind;

/// The output being built over a source's pieces: the lines so far, the
/// class and last source row (absolute) of the previous top-level item, and
/// the rows kept verbatim (absolute, inclusive), one W142 each.
///
/// Nothing is reordered except runs of consecutive imports, which are
/// sorted. Comments stay where they are: a comment on the line of a node
/// stays on that line, and a comment directly above a block stays attached
/// to it. Blocks are separated by exactly one blank line; a blank line
/// between two comments, or between a comment and a block, is kept (one),
/// since it tells a standalone comment from a block's doc comment. Inside
/// blocks blank lines are removed.
pub(super) struct Emitter<'a> {
    source_lines: &'a [&'a str],
    config: &'a FormatConfig,
    out: Vec<String>,
    prev: Option<(Item, usize)>,
    kept: Vec<(usize, usize)>,
}

impl<'a> Emitter<'a> {
    pub(super) fn new(source_lines: &'a [&'a str], config: &'a FormatConfig) -> Self {
        Emitter {
            source_lines,
            config,
            out: Vec::new(),
            prev: None,
            kept: Vec::new(),
        }
    }

    /// Emit one piece's top-level items. An item is kept verbatim when it
    /// overlaps an error region of the piece's tree or holds the piece's
    /// unclosed string. Items that share a source row form a run, and a run
    /// holding a kept item is kept whole, so every source row is written once.
    pub(super) fn piece(&mut self, piece: &Piece<'_>) {
        let text = piece.text();
        let base = piece.row();
        let root = piece.root();
        let error_regions = collect_error_regions(root);
        let broken = |node: Node| overlaps(node, &error_regions) || piece.holds_unclosed(node);
        let mut cursor = root.walk();
        let items: Vec<Node> = root.children(&mut cursor).collect();

        // Items sharing a row form a run; a run holding a broken item is
        // kept whole, so every source row is written once.
        let rows_of = |n: Node| (base + n.start_position().row, base + n.end_position().row);
        let mut run_end = vec![0; items.len()]; // index one past the last item of i's run
        let mut run_broken = vec![false; items.len()];
        let mut run_start = 0;
        while run_start < items.len() {
            let mut end = run_start + 1;
            let mut last_row = rows_of(items[run_start]).1;
            while end < items.len() && rows_of(items[end]).0 <= last_row {
                last_row = last_row.max(rows_of(items[end]).1);
                end += 1;
            }
            let broken_run = items[run_start..end].iter().any(|&n| broken(n));
            for k in run_start..end {
                run_end[k] = end;
                run_broken[k] = broken_run;
            }
            run_start = end;
        }

        let mut i = 0;
        while i < items.len() {
            let node = items[i];
            let class = Item::of(node);
            let start = base + node.start_position().row;
            let end = base + node.end_position().row;

            if run_broken[i] {
                let (first, _) = self.rows(piece, node);
                let (_, last) = self.rows(piece, items[run_end[i] - 1]);
                if let Some((prev_class, prev_end)) = self.prev
                    && needs_blank_line(prev_class, class, first.saturating_sub(prev_end + 1))
                {
                    self.out.push(String::new());
                }
                self.kept.push((first, last));
                self.out.extend(self.lines(first, last));
                self.prev = Some((Item::of(items[run_end[i] - 1]), last));
                i = run_end[i];
                continue;
            }

            // A comment on the line where the previous item ends stays there.
            if class == Item::Comment
                && self.prev.is_some_and(|(_, prev_end)| prev_end == start)
                && let Some(last) = self.out.last_mut()
            {
                last.push(' ');
                last.push_str(&comment_text(node, text));
                self.prev = Some((Item::Comment, end));
                i += 1;
                continue;
            }

            if let Some((prev_class, prev_end)) = self.prev {
                let gap = start.saturating_sub(prev_end + 1);
                if needs_blank_line(prev_class, class, gap) {
                    self.out.push(String::new());
                }
            }

            // A run of imports (blank lines allowed, no comments) is sorted.
            if class == Item::Import {
                let mut run = Vec::new();
                let mut last_end = end;
                while i < items.len() && Item::of(items[i]) == Item::Import && !run_broken[i] {
                    let import = items[i];
                    last_end = base + import.end_position().row;
                    if holds_comment(import) {
                        let (first, last) = self.rows(piece, import);
                        run.push(self.lines(first, last).join("\n"));
                    } else {
                        run.push(one_line(import, text));
                    }
                    i += 1;
                }
                run.sort();
                self.out.extend(run);
                self.prev = Some((Item::Import, last_end));
                continue;
            }

            if class == Item::Comment {
                self.out.push(comment_text(node, text));
            } else {
                self.out.extend(format_block(node, text, self.config));
            }
            self.prev = Some((class, end));
            i += 1;
        }
    }

    /// The rows (absolute, inclusive) of `node` of `piece` as source lines:
    /// from its first row to its last row in the piece's source (the
    /// appended delimiter excluded), trailing blank lines dropped.
    fn rows(&self, piece: &Piece<'_>, node: Node<'_>) -> (usize, usize) {
        let base = piece.row();
        let source_part = &piece.text()[..piece.source_len()];
        let piece_last = base + source_part.trim_end_matches('\n').matches('\n').count();
        let first = (base + node.start_position().row).min(piece_last);
        let mut last = (base + node.end_position().row).min(piece_last);
        while last > first
            && self
                .source_lines
                .get(last)
                .is_none_or(|l| l.trim().is_empty())
        {
            last -= 1;
        }
        (first, last)
    }

    /// The source lines `first..=last`, trailing whitespace trimmed.
    fn lines(&self, first: usize, last: usize) -> Vec<String> {
        (first..=last)
            .filter_map(|row| self.source_lines.get(row).map(|l| l.trim_end().to_string()))
            .collect()
    }

    /// The text (`out` joined, final newline) and one W142 per kept range
    /// (adjacent ranges merged), spanned from its first line's start to its
    /// last line's end.
    pub(super) fn finish(self) -> FormatResult {
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (first, last) in self.kept {
            if let Some(previous) = merged.last_mut()
                && first <= previous.1 + 1
            {
                previous.1 = previous.1.max(last);
                continue;
            }
            merged.push((first, last));
        }
        let diagnostics = merged
            .into_iter()
            .map(|(first, last)| {
                let last_line_len = self.source_lines.get(last).map_or(0, |line| line.len());
                kept_region(first, last, last_line_len)
            })
            .collect();
        let mut formatted = self.out.join("\n");
        if !formatted.is_empty() {
            formatted.push('\n');
        }
        FormatResult {
            formatted,
            diagnostics,
        }
    }
}

/// What a top-level item is, for the blank-line rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Import,
    Comment,
    Block,
}

impl Item {
    fn of(node: Node) -> Self {
        match node.kind() {
            kind::USE_IMPORT | kind::PUB_USE_IMPORT => Item::Import,
            kind::COMMENT => Item::Comment,
            _ => Item::Block,
        }
    }
}

/// Whether one blank line separates `prev` from `next` at top level, given
/// the `gap` of blank lines between them in the source.
fn needs_blank_line(prev: Item, next: Item, gap: usize) -> bool {
    match (prev, next) {
        (Item::Block, _) => true,
        (Item::Import, Item::Import) => false,
        (Item::Import, _) => true,
        // A comment directly above something is attached to it.
        (Item::Comment, _) => gap > 0,
    }
}
