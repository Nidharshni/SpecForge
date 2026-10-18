use std::borrow::Cow;

use crate::diagram::markdown_cell;

/// `text` as a markdown quote, whole: its blank edge lines trimmed, its
/// common leading whitespace removed, every line prefixed `> `.
pub fn quote(text: &str) -> String {
    let lines = dedent(text);
    if lines.is_empty() {
        return ">".to_string();
    }
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text`'s lines without the blank edge lines and the common leading
/// whitespace, each right-trimmed; none for blank text.
pub(crate) fn dedent(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return Vec::new();
    };
    let lines = &lines[first..=last];
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or_else(|| l.trim_start()).trim_end())
        .collect()
}

/// `text` as it renders when written: outside code spans `\`, `<` and `&` are
/// backslash-escaped, so no raw HTML, autolink or entity reference can form.
/// Borrowed when there is nothing to escape.
pub(crate) fn inline(text: &str) -> Cow<'_, str> {
    if !text.contains(['\\', '<', '&']) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 8);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        escape_line(line, &mut out);
    }
    Cow::Owned(out)
}

/// `text` as one table cell: escaped like [`inline`], `|` escaped, one line.
pub(crate) fn cell(text: &str) -> String {
    markdown_cell(&inline(text)).into_owned()
}

/// One line: a code span (a backtick run closed by a run of the same length
/// on the line) is copied as written; a run left open stays literal.
fn escape_line(line: &str, out: &mut String) {
    let bytes = line.as_bytes();
    let run_at = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut i = 0;
    while i < line.len() {
        match bytes[i] {
            b'`' => {
                let n = run_at(i);
                let mut j = i + n;
                let mut end = None;
                while j < line.len() {
                    if bytes[j] == b'`' {
                        let m = run_at(j);
                        if m == n {
                            end = Some(j + m);
                            break;
                        }
                        j += m;
                    } else {
                        j += 1;
                    }
                }
                let stop = end.unwrap_or(i + n);
                out.push_str(&line[i..stop]);
                i = stop;
            }
            b'\\' | b'<' | b'&' => {
                out.push('\\');
                out.push(bytes[i] as char);
                i += 1;
            }
            _ => {
                // Copy up to the next byte that needs a look: a char at a time keeps UTF-8 whole.
                let ch = line[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "export_review_document",
        verify = "prose renders as written, raw HTML escaped outside code spans"
    )]
    fn prose_renders_as_written_raw_html_escaped_outside_code_spans() {
        assert_eq!(
            inline("It MUST return Result<Task, E>."),
            "It MUST return Result\\<Task, E>."
        );
        assert_eq!(
            inline("set status to `done` & go"),
            "set status to `done` \\& go"
        );
        assert_eq!(inline("`a<b` and <b>x</b>"), "`a<b` and \\<b>x\\</b>");
        assert_eq!(inline("``a`b<`` then c<d"), "``a`b<`` then c\\<d");
        assert_eq!(inline("one ` open <x"), "one ` open \\<x");
        assert_eq!(inline(r"C:\dir"), r"C:\\dir");
        assert_eq!(inline("line1 `x\nline2` <y"), "line1 `x\nline2` \\<y");
        assert!(matches!(inline("plain words"), Cow::Borrowed(_)));
    }

    #[test]
    fn a_cell_is_one_escaped_line() {
        assert_eq!(cell("a|b <c>\nd"), "a\\|b \\<c> d");
    }

    #[test]
    fn quote_trims_blank_edges_and_dedents() {
        assert_eq!(quote("\n    a\n      b\n    c\n"), "> a\n>   b\n> c");
    }

    #[test]
    fn an_empty_line_inside_is_a_bare_marker() {
        assert_eq!(quote("a\n\nb"), "> a\n>\n> b");
    }

    #[test]
    fn blank_text_is_one_marker() {
        assert_eq!(quote("  \n "), ">");
    }
}
