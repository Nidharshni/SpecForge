//! What `exclude` in specforge.json leaves out of a project's sources
//! (ADR 0055 D7, D8).

use std::path::{Component, Path};

use glob::MatchOptions;

/// The `exclude` entries of a project: gitignore-style glob patterns over a
/// source's path relative to the spec root, written with `/`.
///
/// - `*` and `?` match within one name, `**` any number of directories,
///   `[...]` one character of a set;
/// - an entry with no `/` except a trailing one matches a name at any depth
///   (`drafts` leaves out `drafts/a.spec` and `sub/drafts/b.spec`);
/// - an entry with a `/` at its start or in its middle is anchored at the
///   spec root (`/drafts`, `sub/drafts`);
/// - a trailing `/` matches directories only;
/// - a file is left out when an entry matches its path or the path of one of
///   its directories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exclude {
    entries: Vec<ExcludeEntry>,
}

/// One usable `exclude` entry: the text written and the pattern it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeEntry {
    written: String,
    pattern: glob::Pattern,
    directories_only: bool,
}

/// Why an `exclude` entry can match no source (E069; the entry is ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeError {
    /// Nothing to match: empty, `/`, or an empty name (`a//b`).
    Empty,
    /// Starts with `!`: `exclude` has no negation.
    Negation,
    /// Has a `.` or `..` name, which no path under the spec root has.
    Dots,
    /// Not a glob pattern: the pattern parser's message.
    Pattern(String),
}

impl std::fmt::Display for ExcludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExcludeError::Empty => write!(f, "matches nothing: it has no name"),
            ExcludeError::Negation => {
                write!(f, "starts with \"!\", and exclude has no negation")
            }
            ExcludeError::Dots => write!(
                f,
                "has a \".\" or \"..\" name, which no path under the spec root has"
            ),
            ExcludeError::Pattern(message) => write!(f, "is not a glob pattern ({message})"),
        }
    }
}

/// Names compare exactly and `*` stays within one name.
const MATCHING: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

impl ExcludeEntry {
    /// `written` as an entry, or why it can match no source.
    pub fn parse(written: &str) -> Result<Self, ExcludeError> {
        if written.starts_with('!') {
            return Err(ExcludeError::Negation);
        }
        let (rest, directories_only) = match written.strip_suffix('/') {
            Some(rest) => (rest, true),
            None => (written, false),
        };
        let anchored = rest.contains('/');
        let rest = if anchored {
            rest.strip_prefix('/').unwrap_or(rest)
        } else {
            rest
        };
        if rest.is_empty() || rest.split('/').any(str::is_empty) {
            return Err(ExcludeError::Empty);
        }
        if rest.split('/').any(|name| name == "." || name == "..") {
            return Err(ExcludeError::Dots);
        }
        // The entry as written is what a syntax error is reported against.
        glob::Pattern::new(rest).map_err(|e| ExcludeError::Pattern(e.to_string()))?;
        let pattern = if anchored {
            rest.to_string()
        } else {
            format!("**/{rest}")
        };
        let pattern =
            glob::Pattern::new(&pattern).map_err(|e| ExcludeError::Pattern(e.to_string()))?;
        Ok(ExcludeEntry {
            written: written.to_string(),
            pattern,
            directories_only,
        })
    }

    /// The entry as specforge.json writes it.
    pub fn written(&self) -> &str {
        &self.written
    }
}

impl Exclude {
    pub fn new(entries: Vec<ExcludeEntry>) -> Self {
        Exclude { entries }
    }

    /// `written`, every entry usable; the first error otherwise (a literal
    /// list in code or a test).
    pub fn parse<'a>(written: impl IntoIterator<Item = &'a str>) -> Result<Self, ExcludeError> {
        written
            .into_iter()
            .map(ExcludeEntry::parse)
            .collect::<Result<Vec<_>, _>>()
            .map(Exclude::new)
    }

    pub fn entries(&self) -> &[ExcludeEntry] {
        &self.entries
    }

    /// Whether an entry leaves out the file at `relative` (its path relative
    /// to the spec root): it matches the path, or the path of one of its
    /// directories. Only a path of plain names can be left out.
    pub fn excludes(&self, relative: &Path) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        let mut names = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(name) => match name.to_str() {
                    Some(name) => names.push(name),
                    None => return false,
                },
                _ => return false,
            }
        }
        (1..=names.len()).any(|end| {
            let prefix = names[..end].join("/");
            let is_directory = end < names.len();
            self.entries.iter().any(|entry| {
                (is_directory || !entry.directories_only)
                    && entry.pattern.matches_with(&prefix, MATCHING)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "an exclude entry that can match no source is E069 and ignored"
    )]
    fn each_unusable_entry_names_why() {
        assert_eq!(ExcludeEntry::parse(""), Err(ExcludeError::Empty));
        assert_eq!(ExcludeEntry::parse("/"), Err(ExcludeError::Empty));
        assert_eq!(ExcludeEntry::parse("a//b"), Err(ExcludeError::Empty));
        assert_eq!(ExcludeEntry::parse("!drafts"), Err(ExcludeError::Negation));
        assert_eq!(ExcludeEntry::parse("./drafts"), Err(ExcludeError::Dots));
        assert_eq!(ExcludeEntry::parse("a/../b"), Err(ExcludeError::Dots));
        assert_eq!(ExcludeEntry::parse(".."), Err(ExcludeError::Dots));
        assert!(matches!(
            ExcludeEntry::parse("a[b"),
            Err(ExcludeError::Pattern(_))
        ));
        assert!(matches!(
            ExcludeEntry::parse("a**b"),
            Err(ExcludeError::Pattern(_))
        ));
        assert_eq!(ExcludeEntry::parse("drafts/").unwrap().written(), "drafts/");
    }

    #[specforge_test(
        behavior = "resolve_use_imports",
        verify = "exclude entries are glob patterns matched against a file and its directories"
    )]
    fn an_entry_matches_a_file_through_its_directories() {
        let exclude = Exclude::parse(["legacy"]).unwrap();
        assert!(exclude.excludes(Path::new("a/legacy/b/c.spec")));
        assert!(!exclude.excludes(Path::new("legacy.spec/x.spec")));
        assert!(exclude.excludes(Path::new("legacy")));
        assert!(!exclude.excludes(Path::new("legacies/c.spec")));

        // A trailing slash is directories only: a file of that name stays.
        let directories = Exclude::parse(["legacy/"]).unwrap();
        assert!(directories.excludes(Path::new("a/legacy/c.spec")));
        assert!(!directories.excludes(Path::new("a/legacy")));

        // Only a path of plain names can be left out.
        assert!(!exclude.excludes(Path::new("../legacy/c.spec")));
        assert!(!exclude.excludes(Path::new("/legacy/c.spec")));
        assert!(!Exclude::default().excludes(Path::new("legacy/c.spec")));
    }
}
