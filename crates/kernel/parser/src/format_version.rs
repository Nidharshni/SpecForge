//! The format version of a `.spec` file: the number its first-line header
//! (`// specforge-format: <major>.<minor>`) declares. It names the file format
//! the file was written against, and is distinct from the Graph Protocol's
//! schema version. A file with no header is at the current version.
//!
//! [`detect_format_version`] reads the header and [`with_format_version`]
//! rewrites it, both through one placement rule: the first non-blank line.
//! The parser attaches what the reader reports to every file it parses (so
//! `check`, watch, the LSP and MCP show it as a compile diagnostic), and
//! `specforge migrate` reads and rewrites files through the two.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, SourceSpan, Sym, codes};

/// The DSL format version embedded in spec file headers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Shape)]
#[shape(define)]
pub struct FormatVersion {
    pub major: u32,
    pub minor: u32,
}

/// Current format version. All new spec files are at this version.
pub const CURRENT_FORMAT_VERSION: FormatVersion = FormatVersion { major: 1, minor: 0 };

/// Minimum supported format version for migration.
pub const MIN_SUPPORTED_VERSION: FormatVersion = FormatVersion { major: 1, minor: 0 };

/// Maximum supported target version.
pub const MAX_SUPPORTED_VERSION: FormatVersion = FormatVersion { major: 1, minor: 0 };

/// What a format version header starts with.
pub const FORMAT_HEADER_PREFIX: &str = HEADER_PREFIX;

const HEADER_PREFIX: &str = "// specforge-format: ";

impl fmt::Display for FormatVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for FormatVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        match parts.len() {
            1 => {
                let major = parts[0]
                    .parse::<u32>()
                    .map_err(|e| format!("invalid version: {e}"))?;
                Ok(FormatVersion { major, minor: 0 })
            }
            2 => {
                let major = parts[0]
                    .parse::<u32>()
                    .map_err(|e| format!("invalid major: {e}"))?;
                let minor = parts[1]
                    .parse::<u32>()
                    .map_err(|e| format!("invalid minor: {e}"))?;
                Ok(FormatVersion { major, minor })
            }
            _ => Err(format!("expected MAJOR.MINOR, got '{s}'")),
        }
    }
}

/// The format version `content` (the text of `file`) declares in its header,
/// and what the header reports. A file with no header is at
/// [`CURRENT_FORMAT_VERSION`] and reports nothing.
///
/// - an older version than the minimum supported: I007, "migration available";
/// - a version newer than the maximum supported: E019, with the range this
///   build supports;
/// - a header that is not `MAJOR.MINOR`: E019, with the expected shape (the
///   version is then [`MIN_SUPPORTED_VERSION`]).
///
/// Each diagnostic is spanned on the header line, so it sits where the file
/// declares the version.
pub fn detect_format_version(content: &str, file: &str) -> (FormatVersion, Vec<Diagnostic>) {
    let Some(header) = header_line(content) else {
        return (CURRENT_FORMAT_VERSION, Vec::new());
    };
    let version_str = &header.text[HEADER_PREFIX.len()..];
    let index = header.index;
    let span = SourceSpan {
        file: Sym::new(file),
        start_line: index + 1,
        start_col: 1,
        end_line: index + 1,
        end_col: header.text.chars().count() + 1,
    };
    let version_str = version_str.trim();
    match FormatVersion::from_str(version_str) {
        Ok(version) if version > MAX_SUPPORTED_VERSION => {
            let diagnostic = Diagnostic::new(
                codes::E019,
                format!(
                    "unsupported format version {version} (max supported: {MAX_SUPPORTED_VERSION})"
                ),
            )
            .with_span(span)
            .with_suggestion(format!(
                "Use a format version between {MIN_SUPPORTED_VERSION} and {MAX_SUPPORTED_VERSION}."
            ));
            (version, vec![diagnostic])
        }
        Ok(version) if version < MIN_SUPPORTED_VERSION => {
            let diagnostic = Diagnostic::new(
                codes::I007,
                format!(
                    "format version {version} is older than current ({CURRENT_FORMAT_VERSION}); migration available"
                ),
            )
            .with_span(span)
            .with_suggestion("Run `specforge migrate` to upgrade.".to_string());
            (version, vec![diagnostic])
        }
        Ok(version) => (version, Vec::new()),
        Err(_) => {
            let diagnostic = Diagnostic::new(
                codes::E019,
                format!("invalid format version header: '{version_str}'"),
            )
            .with_span(span)
            .with_suggestion(format!(
                "Expected `// specforge-format: MAJOR.MINOR` (e.g., `// specforge-format: {CURRENT_FORMAT_VERSION}`)."
            ));
            (MIN_SUPPORTED_VERSION, vec![diagnostic])
        }
    }
}

/// The file's header line: its first non-blank line, when that starts with the prefix.
struct HeaderLine<'a> {
    /// 0-based line index.
    index: usize,
    /// Byte range of the line in the content, its line ending excluded.
    range: std::ops::Range<usize>,
    /// The line's text (the content at `range`).
    text: &'a str,
}

/// The one placement rule: the first line whose `trim()` is non-empty, if it
/// starts with the prefix. Lines split as `str::lines` splits them (on `\n`,
/// a preceding `\r` dropped), with byte offsets.
fn header_line(content: &str) -> Option<HeaderLine<'_>> {
    let mut offset = 0;
    for (index, raw) in content.split_inclusive('\n').enumerate() {
        let text = match raw.strip_suffix('\n') {
            Some(line) => line.strip_suffix('\r').unwrap_or(line),
            None => raw,
        };
        if !text.trim().is_empty() {
            return text.starts_with(HEADER_PREFIX).then(|| HeaderLine {
                index,
                range: offset..offset + text.len(),
                text,
            });
        }
        offset += raw.len();
    }
    None
}

/// `content` declaring `version`: the header line [`header_line`] finds is
/// replaced in place, its line ending kept; a file with none gets
/// `// specforge-format: <version>` as its first line, ended like the file's
/// first line (`\r\n` when that ends so, else `\n`). No other byte changes.
pub fn with_format_version(content: &str, version: &FormatVersion) -> String {
    let header = format!("{HEADER_PREFIX}{version}");
    match header_line(content) {
        Some(found) => format!(
            "{}{header}{}",
            &content[..found.range.start],
            &content[found.range.end..]
        ),
        None => {
            let ending = match content.find('\n') {
                Some(at) if content[..at].ends_with('\r') => "\r\n",
                _ => "\n",
            };
            format!("{header}{ending}{content}")
        }
    }
}
