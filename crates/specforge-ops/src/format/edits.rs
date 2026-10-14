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

    use super::super::{EditorOptions, Lines, Place, document};
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

    /// The corpora `specforge-cli/tests/format_corpus.rs` formats, and the parser's and CLI's fixtures.
    const CORPORA: [&str; 6] = [
        "spec",
        "examples/todo-app",
        "examples/shop",
        "integrations/rust/spec",
        "crates/specforge-parser/tests/fixtures",
        "crates/specforge-cli/tests/fixtures",
    ];

    /// Inputs no corpus file has.
    const EDGES: [&str; 12] = [
        "",
        "\n",
        "\r\n",
        "\n\n\n",
        "// c",
        "// c\r\n\r\n",
        "a\rb\rc",
        "\u{feff}behavior a \"A\" {\n  contract \"x\"\n}\n",
        "behavior a \"é\" {\r\n\r\n      contract \"x\"}",
        "behavior a \"A\" {\n  contract \"unclosed\n}\n",
        "behavior a \"A\" {\n  contract \"x\"\n}\n\n}}}\n",
        "behavior a \"A\" {\n  contract \"\"\"\r\n  x\r\n  \"\"\"\n}",
    ];

    /// A deterministic generator (xorshift64): the same perturbations on every run.
    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn one_in(&mut self, n: u64) -> bool {
            self.next().is_multiple_of(n)
        }
    }

    /// `text` with seeded whitespace noise, per line.
    fn noise(text: &str, seed: u64) -> String {
        let mut rng = Noise(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut out = String::new();
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            if rng.one_in(4) {
                out.push_str("   ");
            }
            out.push_str(line);
            if rng.one_in(5) {
                out.push_str("  ");
            }
            if n + 1 == lines.len() {
                if !rng.one_in(2) {
                    out.push('\n');
                }
                break;
            }
            if rng.one_in(40) {
                out.push(' ');
            } else if rng.one_in(7) {
                out.push_str("\r\n");
            } else {
                out.push('\n');
            }
            if rng.one_in(6) {
                out.push('\n');
            }
        }
        out
    }

    /// `text` laid out ten ways, each named. The invariant holds for any text, so a perturbation may
    /// change a string's content or break the parse (W142 regions): that is coverage too.
    fn perturbations(text: &str, seed: u64) -> Vec<(&'static str, String)> {
        let no_final = text.trim_end_matches(['\n', '\r']).to_string();
        let mut rng = Noise(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
        let mixed: String = text
            .split_inclusive('\n')
            .map(|line| match line.strip_suffix('\n') {
                Some(body) if rng.one_in(3) => format!("{body}\r\n"),
                _ => line.to_string(),
            })
            .collect();
        vec![
            ("identity", text.to_string()),
            (
                "doubled",
                format!("\n\n{}\n\n", text.replace("\n\n", "\n\n\n\n")),
            ),
            (
                "after_brace",
                text.lines()
                    .map(|l| {
                        if l.ends_with('{') {
                            format!("{l}\n\n")
                        } else {
                            format!("{l}\n")
                        }
                    })
                    .collect(),
            ),
            ("no_final", no_final.clone()),
            ("crlf", text.replace('\n', "\r\n")),
            (
                "tabs",
                text.lines()
                    .map(|l| {
                        let spaces = l.len() - l.trim_start_matches(' ').len();
                        format!("{}{}\n", "\t".repeat(spaces / 2), &l[spaces / 2 * 2..])
                    })
                    .collect(),
            ),
            (
                "reindented",
                text.lines()
                    .map(|l| {
                        if l.trim().is_empty() {
                            format!("{l}\n")
                        } else {
                            format!("   {l}  \n")
                        }
                    })
                    .collect(),
            ),
            ("mixed", mixed),
            ("noise", noise(text, seed)),
            ("combo", no_final.replace('\n', "\r\n")),
        ]
    }

    /// One laid-out input: where it came from and its text.
    struct Input {
        name: String,
        corpus: usize,
        text: String,
    }

    /// Every corpus file under every perturbation, then the edge inputs.
    fn corpus_inputs() -> Vec<Input> {
        let workspace = specforge_repo_root::repo_root();
        let mut inputs = Vec::new();
        let mut seed = 0;
        for (corpus, dir) in CORPORA.iter().enumerate() {
            for file in specforge_common::walk_spec_files(&workspace.join(dir), |_| false).files {
                let Ok(text) = std::fs::read_to_string(&file) else {
                    continue;
                };
                seed += 1;
                for (layout, text) in perturbations(&text, seed) {
                    inputs.push(Input {
                        name: format!("{} {layout}", file.display()),
                        corpus,
                        text,
                    });
                }
            }
        }
        for (n, edge) in EDGES.iter().enumerate() {
            inputs.push(Input {
                name: format!("edge {n}"),
                corpus: usize::MAX,
                text: (*edge).to_string(),
            });
        }
        inputs
    }

    /// Whether `edits` hold the shape `FormattedDocument::edits` promises over `source`.
    fn well_formed(source: &str, edits: &[Edit]) -> Result<(), String> {
        for (n, edit) in edits.iter().enumerate() {
            let Range { start, end } = edit.range.clone();
            if start > end || end > source.len() {
                return Err(format!(
                    "edit {n}: {start}..{end} is no range of the source"
                ));
            }
            for at in [start, end] {
                if !source.is_char_boundary(at) {
                    return Err(format!("edit {n}: {at} is inside a character"));
                }
                if splits_crlf(source, at) {
                    return Err(format!("edit {n}: {at} is inside a \\r\\n"));
                }
            }
            if source[start..end] == edit.new_text {
                return Err(format!("edit {n}: a no-op"));
            }
            if let Some(next) = edits.get(n + 1)
                && end >= next.range.start
            {
                return Err(format!("edit {n}: overlaps or touches edit {}", n + 1));
            }
        }
        Ok(())
    }

    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "applying the formatting edits to the document gives the formatted text byte for byte"
    )]
    fn the_edits_rebuild_the_formatted_text_over_the_corpus() {
        let editors = [
            None,
            Some(EditorOptions {
                tab_size: 4,
                insert_spaces: false,
            }),
        ];
        let mut failures = Vec::new();
        let mut checked = 0;
        for input in corpus_inputs() {
            for editor in editors {
                let doc = document(Place::Detached, &input.text, None, editor);
                let edits = doc.edits();
                checked += 1;
                let shape = well_formed(&input.text, &edits);
                if apply(&input.text, &edits) != doc.formatted
                    || shape.is_err()
                    || edits.is_empty() == doc.changed()
                {
                    failures.push(format!("{} {editor:?}: {shape:?}", input.name));
                }
            }
        }
        assert!(checked > 4000, "{checked}");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[specforge_test(
        behavior = "lsp_format_range",
        verify = "range formatting changes no byte outside the blocks it touches"
    )]
    #[specforge_test(
        behavior = "lsp_format_document",
        verify = "applying the formatting edits to the document gives the formatted text byte for byte"
    )]
    fn a_range_formats_edits_rebuild_its_text_over_the_corpus() {
        let mut failures = Vec::new();
        let mut checked = 0;
        for input in corpus_inputs() {
            if input.corpus == usize::MAX
                || !["reindented", "crlf", "noise"]
                    .iter()
                    .any(|layout| input.name.ends_with(layout))
            {
                continue;
            }
            // Every 5th line, every 40th under `spec/`.
            let step = if input.corpus == 0 { 40 } else { 5 };
            for line in (0..input.text.lines().count()).step_by(step) {
                let lines = Lines {
                    first: line,
                    last: line,
                };
                let doc = document(Place::Detached, &input.text, Some(lines), None);
                let edits = doc.edits();
                checked += 1;
                let shape = well_formed(&input.text, &edits);
                if apply(&input.text, &edits) != doc.formatted
                    || shape.is_err()
                    || edits.is_empty() == doc.changed()
                {
                    failures.push(format!("{} line {line}: {shape:?}", input.name));
                }
            }
        }
        assert!(checked > 1000, "{checked}");
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
