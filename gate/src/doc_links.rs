//! `doc-links`: every relative link in a tracked Markdown file names a tracked file or directory (ADR 0082).
//!
//! A link is a Markdown `](target)` or an HTML `href="target"` / `src="target"`, outside fenced code blocks
//! and inline code spans. URLs (`scheme:`), `//host` links and in-page `#anchors` are not judged. The
//! fragment and query are dropped, and the path is resolved against the linking file's directory (a leading
//! `/` is the repository root, as GitHub resolves it). Anchors themselves are not checked.

use std::collections::BTreeSet;
use std::fmt;

/// A relative link whose target the repository does not track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broken {
    /// The linking file, repo-relative and `/`-separated.
    pub file: String,
    /// 1-based.
    pub line: usize,
    /// The target as written.
    pub target: String,
}

impl fmt::Display for Broken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} links `{}`, which is not tracked",
            self.file, self.line, self.target
        )
    }
}

/// The relative links of the Markdown files in `tracked` (`git ls-files`) whose target is neither a tracked
/// file nor a directory that holds one. `read` gives a file's text; a file it cannot read is skipped.
pub fn broken(tracked: &[String], read: impl Fn(&str) -> Option<String>) -> Vec<Broken> {
    let mut known: BTreeSet<&str> = BTreeSet::from([""]);
    for path in tracked {
        known.insert(path);
        let mut rest = path.as_str();
        while let Some(slash) = rest.rfind('/') {
            rest = &rest[..slash];
            known.insert(rest);
        }
    }
    let mut found = Vec::new();
    for file in tracked.iter().filter(|p| p.ends_with(".md")) {
        let Some(text) = read(file) else { continue };
        for (line, target) in links(&text) {
            let tracked = resolve(file, &target).is_some_and(|path| known.contains(path.as_str()));
            if !tracked {
                found.push(Broken {
                    file: file.clone(),
                    line,
                    target,
                });
            }
        }
    }
    found
}

/// `(line, target)` for each relative link of a Markdown text.
fn links(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut fence: Option<&str> = None;
    for (index, line) in text.lines().enumerate() {
        let start = line.trim_start();
        if let Some(mark) = fence {
            if start.starts_with(mark) {
                fence = None;
            }
            continue;
        }
        if let Some(mark) = ["```", "~~~"].into_iter().find(|m| start.starts_with(m)) {
            fence = Some(mark);
            continue;
        }
        for target in targets(&without_code_spans(line)) {
            if is_relative(&target) {
                found.push((index + 1, target));
            }
        }
    }
    found
}

/// `line` without its inline code spans: a backtick, the text up to the next backtick, and that backtick.
fn without_code_spans(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let Some(close) = rest[open + 1..].find('`') else {
            break;
        };
        out.push_str(&rest[..open]);
        rest = &rest[open + 1 + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The targets of `](…)` (with an optional `<…>` and `"title"`), `href="…"` and `src="…"` in one line.
fn targets(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in line.match_indices("](") {
        let rest = line[at + 2..].trim_start();
        let rest = rest.strip_prefix('<').unwrap_or(rest);
        let end = rest
            .find(|c: char| c == ')' || c == '>' || c.is_whitespace())
            .unwrap_or(rest.len());
        let tail = rest[end..]
            .strip_prefix('>')
            .unwrap_or(&rest[end..])
            .trim_start();
        let tail = match tail.strip_prefix('"') {
            Some(title) => title
                .split_once('"')
                .map_or("", |(_, after)| after)
                .trim_start(),
            None => tail,
        };
        if end > 0 && tail.starts_with(')') {
            found.push(rest[..end].to_string());
        }
    }
    for attribute in ["href=\"", "src=\""] {
        for (at, _) in line.match_indices(attribute) {
            let rest = &line[at + attribute.len()..];
            if let Some(end) = rest.find('"') {
                found.push(rest[..end].to_string());
            }
        }
    }
    found
}

/// Not a URL (`scheme:`), not `//host`, not an in-page `#anchor`.
fn is_relative(target: &str) -> bool {
    if target.is_empty() || target.starts_with('#') || target.starts_with("//") {
        return false;
    }
    let scheme = target.split_once(':').is_some_and(|(head, _)| {
        head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && head
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    !scheme
}

/// The repo-relative path `target` names from `file`, without fragment or query; `None` above the root.
fn resolve(file: &str, target: &str) -> Option<String> {
    let path = target.split(['#', '?']).next().unwrap_or("");
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        let mut dir: Vec<&str> = file.split('/').collect();
        dir.pop();
        dir
    };
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests;
