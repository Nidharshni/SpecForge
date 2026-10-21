//! A block's doc comment: the run of whole-line comments directly above its first line, the format header never one
//! (ADR 0202).

use super::*;

/// Each of `entities` given its doc comment, read from `comments` (whole-line comments between top-level items, in
/// source order, absolute spans) and `header` (the format header's 0-based row, ADR 0095).
pub(super) fn attach(entities: &mut [Entity], comments: &[SourceSpan], header: Option<usize>) {
    for entity in entities {
        let below = comments.partition_point(|c| c.start_line < entity.span.start_line);
        let run = comments[..below]
            .iter()
            .rev()
            .zip((1..entity.span.start_line).rev())
            .take_while(|(c, line)| c.start_line == *line && header != Some(line - 1))
            .count();
        entity.doc_span = (run > 0).then(|| {
            let (first, last) = (&comments[below - run], &comments[below - 1]);
            SourceSpan {
                file: first.file,
                start_line: first.start_line,
                start_col: first.start_col,
                end_line: last.end_line,
                end_col: last.end_col,
            }
        });
    }
}
