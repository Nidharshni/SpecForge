//! A text edit: a span of a spec file's text and the text it becomes. Navigation's fixes and rename's
//! plan answer them; the LSP converts each span to a UTF-16 range, MCP renders it as the spec's
//! `TextEdit`, and [`apply_edits`] is the one applier (ADR 0164).

use std::sync::Arc;

use specforge_common::SourceSpan;

use super::text::SourceText;

/// One edit: `span`'s text becomes `new_text` (an empty span inserts). `span` is a position in the text the
/// project was compiled from: 1-based lines, 1-based byte columns, end exclusive (ADR 0016 D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub span: SourceSpan,
    pub new_text: String,
}

/// `text` with `edits` applied, every span read in `text` (the caller passes one file's edits: those whose
/// `span.file` is that file). Insertions at one position land in the order given. `None` when a span does not lie in
/// `text`, an end falls inside a character, or two edits overlap: a set of edits applies whole or not at all.
pub fn apply_edits<'a>(
    text: &str,
    edits: impl IntoIterator<Item = &'a TextEdit>,
) -> Option<String> {
    let source = SourceText::new(Arc::from(text));
    let mut ranges = edits
        .into_iter()
        .map(|edit| {
            let range = source.range(&edit.span)?;
            (text.is_char_boundary(range.start) && text.is_char_boundary(range.end))
                .then_some((range, edit.new_text.as_str()))
        })
        .collect::<Option<Vec<_>>>()?;
    // Stable: insertions at one position keep the order given once applied right to left.
    ranges.sort_by_key(|(range, _)| (range.start, range.end));
    if ranges
        .windows(2)
        .any(|pair| pair[0].0.end > pair[1].0.start)
    {
        return None;
    }
    let mut out = text.to_string();
    for (range, new_text) in ranges.into_iter().rev() {
        out.replace_range(range, new_text);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::Sym;

    fn span(start: (usize, usize), end: (usize, usize)) -> SourceSpan {
        SourceSpan {
            file: Sym::new("a.spec"),
            start_line: start.0,
            start_col: start.1,
            end_line: end.0,
            end_col: end.1,
        }
    }

    fn edit(line: usize, start_col: usize, end_col: usize, new_text: &str) -> TextEdit {
        TextEdit {
            span: span((line, start_col), (line, end_col)),
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn several_edits_on_one_line_keep_their_columns() {
        let text = "uses [ab, ab]\nab\n";
        // Given left to right; applied right to left.
        let edits = [edit(1, 7, 9, "alpha"), edit(1, 11, 13, "alpha")];
        assert_eq!(
            apply_edits(text, &edits).as_deref(),
            Some("uses [alpha, alpha]\nab\n")
        );
    }

    #[test]
    fn byte_columns_after_multibyte_text_land_on_the_identifier() {
        // "é" and "→" are 2 and 3 bytes: the columns are byte offsets.
        let text = "title \"é→\" ab\n";
        let start = text.find("ab").unwrap() + 1;
        let edits = [edit(1, start, start + 2, "beta")];
        assert_eq!(
            apply_edits(text, &edits).as_deref(),
            Some("title \"é→\" beta\n")
        );
    }

    #[test]
    fn edits_on_other_lines_leave_the_rest_and_a_missing_newline_alone() {
        let text = "ab\nkeep\nab";
        let edits = [edit(3, 1, 3, "gamma")];
        assert_eq!(
            apply_edits(text, &edits).as_deref(),
            Some("ab\nkeep\ngamma")
        );
    }

    #[test]
    fn an_edit_that_does_not_fit_applies_nothing() {
        let text = "ab\nkeep\nab";
        assert_eq!(apply_edits(text, &[edit(9, 1, 3, "never")]), None);
        assert_eq!(apply_edits(text, &[edit(1, 1, 5, "never")]), None);
        assert_eq!(apply_edits("é\n", &[edit(1, 2, 3, "x")]), None);
        let overlapping = [edit(1, 1, 3, "x"), edit(1, 2, 3, "y")];
        assert_eq!(apply_edits(text, &overlapping), None);
        // One that does not fit voids the ones that do.
        assert_eq!(
            apply_edits(text, &[edit(3, 1, 3, "ok"), edit(9, 1, 3, "never")]),
            None
        );
    }

    #[test]
    fn insertions_at_one_position_keep_their_order() {
        let edits = [edit(1, 2, 2, "x"), edit(1, 2, 2, "y")];
        assert_eq!(apply_edits("ab\n", &edits).as_deref(), Some("axyb\n"));
        // A fix's stub edits cross lines.
        let across = [TextEdit {
            span: span((1, 2), (2, 1)),
            new_text: "-".to_string(),
        }];
        assert_eq!(apply_edits("ab\ncd\n", &across).as_deref(), Some("a-cd\n"));
    }
}
