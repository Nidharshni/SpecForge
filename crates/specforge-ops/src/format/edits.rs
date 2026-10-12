//! How a formatted document's edits are built: a line diff of the source and the formatted text
//! (Myers, `similar`, the diff `specforge format --diff` prints), each changed run of lines one edit,
//! trimmed to the bytes it alters (ADR 0021 D8).

use std::ops::Range;

use similar::{DiffTag, TextDiff};

use super::Edit;

/// The edits that turn `source` into `formatted` (see [`super::FormattedDocument::edits`]).
pub(super) fn between(source: &str, formatted: &str) -> Vec<Edit> {
    if source == formatted {
        return Vec::new();
    }
    // Line tokens keep their terminator ("\n", "\r\n" or a lone "\r"), so a change of line break is a
    // changed line, and the tokens concatenate back to the text.
    let diff = TextDiff::from_lines(source, formatted);
    let old = starts(diff.old_slices());
    let new = starts(diff.new_slices());
    let mut edits = Vec::new();
    let mut hunk: Option<(Range<usize>, Range<usize>)> = None;
    // Where each op is comes from the ops' lengths, in order: after compaction, an insert's old index
    // (and a delete's new index) is where compaction found it, not where the script is.
    let (mut old_at, mut new_at) = (0, 0);
    for op in diff.ops() {
        let (tag, o, n) = op.as_tag_tuple();
        let o = old_at..old_at + o.len();
        let n = new_at..new_at + n.len();
        (old_at, new_at) = (o.end, n.end);
        if tag == DiffTag::Equal {
            if let Some((o, n)) = hunk.take() {
                edits.push(trimmed(
                    source,
                    old[o.start]..old[o.end],
                    &formatted[new[n.start]..new[n.end]],
                ));
            }
            continue;
        }
        hunk = Some(match hunk.take() {
            Some((ho, hn)) => (ho.start..o.end, hn.start..n.end),
            None => (o, n),
        });
    }
    if let Some((o, n)) = hunk {
        edits.push(trimmed(
            source,
            old[o.start]..old[o.end],
            &formatted[new[n.start]..new[n.end]],
        ));
    }
    edits
}

/// The byte offset where each line token starts, then the text's length.
fn starts(lines: &[&str]) -> Vec<usize> {
    let mut at = Vec::with_capacity(lines.len() + 1);
    at.push(0);
    for line in lines {
        at.push(at[at.len() - 1] + line.len());
    }
    at
}

/// `source[range]` becoming `new_text`, without the characters both share at either end, and with
/// neither end between a `\r` and its `\n`.
fn trimmed(source: &str, range: Range<usize>, new_text: &str) -> Edit {
    let old = &source[range.clone()];
    let mut prefix: usize = old
        .chars()
        .zip(new_text.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    let mut suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new_text[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    if splits_crlf(source, range.start + prefix) {
        prefix -= 1;
    }
    if splits_crlf(source, range.end - suffix) {
        suffix -= 1;
    }
    Edit {
        range: range.start + prefix..range.end - suffix,
        new_text: new_text[prefix..new_text.len() - suffix].to_string(),
    }
}

/// Whether byte `at` of `source` lies between a `\r` and the `\n` after it.
fn splits_crlf(source: &str, at: usize) -> bool {
    let bytes = source.as_bytes();
    at > 0 && bytes.get(at - 1) == Some(&b'\r') && bytes.get(at) == Some(&b'\n')
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    /// `source` with `edits` applied the way an editor does: every range read in `source`.
    pub(crate) fn apply(source: &str, edits: &[Edit]) -> String {
        let mut out = source.to_string();
        for edit in edits.iter().rev() {
            out.replace_range(edit.range.clone(), &edit.new_text);
        }
        out
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "applying the formatting edits to the document gives the formatted text byte for byte"
    )]
    fn a_crlf_is_never_split() {
        let edits = between("a\r\nb\r\n", "a\nb\n");
        assert_eq!(
            edits,
            [Edit {
                range: 1..6,
                new_text: "\nb\n".into()
            }]
        );
        assert_eq!(apply("a\r\nb\r\n", &edits), "a\nb\n");
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "applying the formatting edits to the document gives the formatted text byte for byte"
    )]
    fn trimming_stops_at_a_char_boundary() {
        // `é` = C3 A9, `è` = C3 A8: a byte-wise prefix would end inside the char.
        let edits = between("x \"é\"\n", "x \"è\"\n");
        assert_eq!(
            edits,
            [Edit {
                range: 3..5,
                new_text: "è".into()
            }]
        );
    }
}
