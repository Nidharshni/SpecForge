/// `text` as a markdown quote, whole: its blank edge lines trimmed, its
/// common leading whitespace removed, every line prefixed `> `.
pub fn quote(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return ">".to_string();
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
        .map(|l| {
            let line = l.get(indent..).unwrap_or_else(|| l.trim_start()).trim_end();
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

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
