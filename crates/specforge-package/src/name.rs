//! What an extension package is called and which of its versions is asked for (ADR 0036).
//!
//! A string enters as one of these types where it enters (the `add` argument, a `specforge.json`
//! entry, a lock entry, a declaration being installed or published, a registry URL) and is passed
//! as the type from then on. No other code splits `name@version` or decides what a name may hold.
//!
//! The module is pure: no I/O and no diagnostics. Callers map its errors to their codes.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use specforge_protocol_types::ExtensionDeclaration;

/// The longest package name, in bytes (npm's limit).
pub const MAX_NAME_LEN: usize = 214;

/// An extension's package name: `@scope/name`, or `name` alone (a local module; a registry holds
/// only scoped names). Each part is one or more of `a-z 0-9 . _ -` and starts with `a-z` or `0-9`,
/// so a name is always a relative path of one or two normal components (never `.`, `..`, absolute
/// or empty) and needs no escaping in a URL except its one `/`.
///
/// Serialized as its text; deserializing text that is not a name fails (`specforge.lock`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackageName {
    text: String,
    /// Byte index of the `/` of a scoped name.
    slash: Option<usize>,
}

impl PackageName {
    /// The name `declaration` declares, read as a package name: `add` and `publish` require one
    /// (ADR 0036); the load does not, so a guest may declare any text.
    pub fn declared_by(declaration: &ExtensionDeclaration) -> Result<Self, PackageNameError> {
        Self::parse(declaration.name())
    }

    /// Read `text` (no trimming: callers trim what users type).
    pub fn parse(text: &str) -> Result<Self, PackageNameError> {
        if text.is_empty() {
            return Err(PackageNameError::Empty);
        }
        if text.len() > MAX_NAME_LEN {
            return Err(PackageNameError::TooLong {
                text: text.to_string(),
            });
        }
        let scoped = text.starts_with('@');
        let parts: Vec<&str> = if scoped {
            text[1..].split('/').collect()
        } else {
            text.split('/').collect()
        };
        if scoped && parts.len() == 1 {
            return Err(PackageNameError::MissingPart {
                text: text.to_string(),
            });
        }
        for part in &parts {
            if part.is_empty() {
                return Err(PackageNameError::MissingPart {
                    text: text.to_string(),
                });
            }
            if !is_part(part) {
                return Err(PackageNameError::BadPart {
                    text: text.to_string(),
                    part: (*part).to_string(),
                });
            }
        }
        if parts.len() > if scoped { 2 } else { 1 } {
            return Err(PackageNameError::TooManyParts {
                text: text.to_string(),
            });
        }
        Ok(PackageName {
            text: text.to_string(),
            slash: scoped.then(|| text.find('/').expect("a scoped name has a slash")),
        })
    }

    /// Read the name a registry URL path carries: `@scope%2Fname` (`%2f` too) or `name`.
    pub fn from_url_segment(segment: &str) -> Result<Self, PackageNameError> {
        Self::parse(&segment.replace("%2F", "/").replace("%2f", "/"))
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// `@scope` of a scoped name, with its `@` (the namespace a publish claims, the
    /// `scope_filter` a registry is chosen by); `None` for a local name.
    pub fn scope(&self) -> Option<&str> {
        self.slash.map(|slash| &self.text[..slash])
    }

    /// The part after the scope (`tool` of `@acme/tool`); the whole of a local name.
    pub fn base(&self) -> &str {
        match self.slash {
            Some(slash) => &self.text[slash + 1..],
            None => &self.text,
        }
    }

    /// The name as one URL path segment: `@acme%2Ftool`.
    pub fn url_segment(&self) -> String {
        self.text.replace('/', "%2F")
    }

    /// The name as a relative path (`@acme/tool` is `@acme` then `tool`): what the installed
    /// layout joins under `.specforge/extensions`. Never escapes the directory it is joined to.
    pub fn relative_path(&self) -> PathBuf {
        match self.scope() {
            Some(scope) => PathBuf::from(scope).join(self.base()),
            None => PathBuf::from(&self.text),
        }
    }
}

/// One part of a name: `a-z 0-9 . _ -`, starting with `a-z` or `0-9`.
fn is_part(part: &str) -> bool {
    let mut chars = part.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

impl FromStr for PackageName {
    type Err = PackageNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl TryFrom<String> for PackageName {
    type Error = PackageNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<PackageName> for String {
    fn from(name: PackageName) -> String {
        name.text
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

/// Why text is not a package name. `Display` is the sentence a diagnostic carries, naming the
/// text: "'@acme/..' is not a package name: a name part starts with a lowercase letter or digit".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageNameError {
    Empty,
    TooLong {
        text: String,
    },
    /// `@scope` with no `/name`, `@/name`, `@scope/`.
    MissingPart {
        text: String,
    },
    /// More than one `/`, or a `/` in an unscoped name.
    TooManyParts {
        text: String,
    },
    /// A part with a character outside `a-z 0-9 . _ -`, or one that starts with `.`, `_` or `-`.
    BadPart {
        text: String,
        part: String,
    },
}

impl fmt::Display for PackageNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageNameError::Empty => f.write_str("'' is not a package name: it is empty"),
            PackageNameError::TooLong { text } => write!(
                f,
                "'{}' is not a package name: it is longer than {MAX_NAME_LEN} bytes",
                text.chars().take(40).collect::<String>() + "..."
            ),
            PackageNameError::MissingPart { text } => write!(
                f,
                "'{text}' is not a package name: a scoped name is @scope/name, each part non-empty"
            ),
            PackageNameError::TooManyParts { text } => write!(
                f,
                "'{text}' is not a package name: a name has at most one '/', after its @scope"
            ),
            PackageNameError::BadPart { text, part } => {
                let starts_well = part
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
                let why = if starts_well {
                    "a name part holds only lowercase letters, digits, '.', '_' and '-'"
                } else {
                    "a name part starts with a lowercase letter or digit"
                };
                write!(f, "'{text}' is not a package name: {why}")
            }
        }
    }
}

impl std::error::Error for PackageNameError {}

/// Which version of a package is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionRequirement {
    /// `latest` (or `*`): the highest release; the highest pre-release only when the package
    /// has no release.
    Latest,
    /// A full version with no operator (`1.2.0`, `2.0.0-rc.1`, `1.0.0+build.5`): that version only,
    /// fetched without listing the package's versions.
    Exact(Version),
    /// Any other SemVer requirement, read as Cargo reads one (`^1.2`, `~1`, `>=1, <2`, `1.x`, and
    /// `1.2`, which is `^1.2`): the highest published version it matches. A pre-release matches only
    /// when the requirement names a pre-release of the same `MAJOR.MINOR.PATCH` (semver's rule).
    Range(VersionReq),
}

impl VersionRequirement {
    /// Read the part after `@` (no trimming). `""` is refused: `name@` means nothing.
    pub fn parse(text: &str) -> Result<Self, VersionRequirementError> {
        let refused = |reason: String| VersionRequirementError {
            text: text.to_string(),
            reason,
        };
        if text.is_empty() {
            return Err(refused("it is empty".to_string()));
        }
        if text == "latest" || text == "*" {
            return Ok(VersionRequirement::Latest);
        }
        if let Ok(version) = Version::parse(text) {
            return Ok(VersionRequirement::Exact(version));
        }
        VersionReq::parse(text)
            .map(VersionRequirement::Range)
            .map_err(|error| refused(error.to_string()))
    }

    /// What `update` asks for when it keeps the major version: `^current`.
    pub fn compatible_with(current: &Version) -> Self {
        VersionRequirement::Range(VersionReq {
            comparators: vec![semver::Comparator {
                op: semver::Op::Caret,
                major: current.major,
                minor: Some(current.minor),
                patch: Some(current.patch),
                pre: current.pre.clone(),
            }],
        })
    }

    /// The version this asks for without listing the package: `Some` for `Exact`.
    pub fn exact(&self) -> Option<&Version> {
        match self {
            VersionRequirement::Exact(version) => Some(version),
            _ => None,
        }
    }

    /// The best of `published` this asks for, `None` when none qualifies. The one resolution
    /// rule: ops `add`, `update` and every test fake call it.
    pub fn pick<'a>(&self, published: &'a [Version]) -> Option<&'a Version> {
        match self {
            VersionRequirement::Latest => published
                .iter()
                .filter(|version| version.pre.is_empty())
                .max()
                .or_else(|| published.iter().max()),
            VersionRequirement::Exact(wanted) => {
                published.iter().find(|version| *version == wanted)
            }
            VersionRequirement::Range(requirement) => published
                .iter()
                .filter(|version| requirement.matches(version))
                .max(),
        }
    }
}

impl fmt::Display for VersionRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionRequirement::Latest => f.write_str("latest"),
            VersionRequirement::Exact(version) => write!(f, "{version}"),
            VersionRequirement::Range(requirement) => write!(f, "{requirement}"),
        }
    }
}

/// Why text is not a version requirement: names the text and semver's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRequirementError {
    pub text: String,
    pub reason: String,
}

impl fmt::Display for VersionRequirementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid version requirement '{}': {}",
            self.text, self.reason
        )
    }
}

impl std::error::Error for VersionRequirementError {}

/// A registry package and the version asked for: `@scope/name` or `@scope/name@<requirement>`,
/// as `specforge add` takes it. The name is always scoped (a registry holds no other).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRef {
    pub name: PackageName,
    pub requirement: VersionRequirement,
}

impl PackageRef {
    /// Read `text` (trimmed by the caller). The requirement is what follows the last `@` that is
    /// not the first character ([`split_requirement`]); none means `Latest`.
    pub fn parse(text: &str) -> Result<Self, SpecifierError> {
        let (name, requirement) = split_requirement(text);
        let name = PackageName::parse(name).map_err(SpecifierError::Name)?;
        if name.scope().is_none() {
            return Err(SpecifierError::Unscoped(name));
        }
        let requirement = match requirement {
            None => VersionRequirement::Latest,
            Some("") => {
                return Err(SpecifierError::EmptyRequirement {
                    text: text.to_string(),
                });
            }
            Some(requirement) => {
                VersionRequirement::parse(requirement).map_err(SpecifierError::Requirement)?
            }
        };
        Ok(PackageRef { name, requirement })
    }
}

impl fmt::Display for PackageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.requirement {
            VersionRequirement::Latest => write!(f, "{}", self.name),
            _ => write!(f, "{}@{}", self.name, self.requirement),
        }
    }
}

/// Why text is not a package reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecifierError {
    Name(PackageNameError),
    /// A valid name with no scope: registry packages are named `@scope/name`.
    Unscoped(PackageName),
    /// `name@` with nothing after the `@`.
    EmptyRequirement {
        text: String,
    },
    Requirement(VersionRequirementError),
}

impl fmt::Display for SpecifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecifierError::Name(error) => error.fmt(f),
            SpecifierError::Unscoped(name) => write!(
                f,
                "'{name}' is not a registry package name: registry packages are named @scope/name"
            ),
            SpecifierError::EmptyRequirement { text } => {
                write!(f, "'{text}' names no version after its '@'")
            }
            SpecifierError::Requirement(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SpecifierError {}

/// The one rule that splits `name@requirement`: at the last `@` that is not the first byte.
/// `("@acme/tool", Some("^1.2"))` for `@acme/tool@^1.2`, `("@acme/tool", None)` for `@acme/tool`,
/// `("@acme/tool", Some(""))` for `@acme/tool@`. `PackageRef::parse` and `ExtensionEntry::parse`
/// read through it; nothing else calls `rfind('@')`.
pub fn split_requirement(text: &str) -> (&str, Option<&str>) {
    match text.rfind('@') {
        Some(at) if at > 0 => (&text[..at], Some(&text[at + 1..])),
        _ => (text, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    /// The accepted names of the plan's table.
    const ACCEPTED: &[&str] = &[
        "@acme/tool",
        "@a/x",
        "@a/pkg_1",
        "@a_b/c",
        "@specforge/cargo-test",
        "greet",
        "x.y",
    ];

    fn variant(error: &PackageNameError) -> &'static str {
        match error {
            PackageNameError::Empty => "Empty",
            PackageNameError::TooLong { .. } => "TooLong",
            PackageNameError::MissingPart { .. } => "MissingPart",
            PackageNameError::TooManyParts { .. } => "TooManyParts",
            PackageNameError::BadPart { .. } => "BadPart",
        }
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "a package name is @scope/name or a local name, and always a relative path inside its directory"
    )]
    fn a_name_is_scoped_or_local_and_a_safe_path() {
        for name in ACCEPTED {
            let parsed = PackageName::parse(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(parsed.as_str(), *name);
        }
        let long = "a".repeat(MAX_NAME_LEN + 1);
        let refused: Vec<(&str, &str)> = vec![
            ("", "Empty"),
            ("@", "MissingPart"),
            ("@acme", "MissingPart"),
            ("@acme/", "MissingPart"),
            ("@/x", "MissingPart"),
            ("@acme/a/b", "TooManyParts"),
            ("a/b", "TooManyParts"),
            ("@acme/..", "BadPart"),
            ("@acme/.x", "BadPart"),
            ("..", "BadPart"),
            (".", "BadPart"),
            ("../../../outside1", "BadPart"),
            ("@acme/T", "BadPart"),
            ("@acme/a b", "BadPart"),
            ("@acme/a#b", "BadPart"),
            ("@acme/a@b", "BadPart"),
            ("@acme/a\\b", "BadPart"),
            (&long, "TooLong"),
        ];
        for (text, want) in refused {
            let error = PackageName::parse(text).expect_err(text);
            assert_eq!(variant(&error), want, "{text:?}");
        }
        assert_eq!(
            PackageName::parse("../../../outside1")
                .unwrap_err()
                .to_string(),
            "'../../../outside1' is not a package name: a name part starts with a lowercase letter or digit"
        );
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "a package name is @scope/name or a local name, and always a relative path inside its directory"
    )]
    fn relative_path_never_leaves_its_directory() {
        use std::path::Component;
        for name in ACCEPTED {
            let name = PackageName::parse(name).unwrap();
            let path = name.relative_path();
            let components: Vec<_> = path.components().collect();
            assert!(
                (1..=2).contains(&components.len())
                    && components.iter().all(|c| matches!(c, Component::Normal(_))),
                "{name}: {path:?}"
            );
            assert_eq!(
                path.to_str().unwrap(),
                name.as_str().replace('/', std::path::MAIN_SEPARATOR_STR)
            );
        }
        let tool = PackageName::parse("@acme/tool").unwrap();
        assert_eq!((tool.scope(), tool.base()), (Some("@acme"), "tool"));
        let local = PackageName::parse("greet").unwrap();
        assert_eq!((local.scope(), local.base()), (None, "greet"));
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "a package name crosses a registry URL as one segment"
    )]
    fn url_segment_round_trips() {
        for name in ACCEPTED {
            let name = PackageName::parse(name).unwrap();
            let segment = name.url_segment();
            assert!(!segment.contains('/'), "{segment}");
            assert_eq!(PackageName::from_url_segment(&segment).unwrap(), name);
        }
        assert_eq!(
            PackageName::parse("@acme/tool").unwrap().url_segment(),
            "@acme%2Ftool"
        );
        assert_eq!(
            PackageName::from_url_segment("@acme%2ftool")
                .unwrap()
                .as_str(),
            "@acme/tool"
        );
        for refused in ["@acme%2F..", "@acme%2FT%20ool", "@acme%2Ftool%2Fx"] {
            assert!(PackageName::from_url_segment(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_name_is_serialized_as_its_text_and_read_only_when_it_is_one() {
        let name = PackageName::parse("@acme/tool").unwrap();
        assert_eq!(serde_json::to_string(&name).unwrap(), "\"@acme/tool\"");
        assert_eq!(
            serde_json::from_str::<PackageName>("\"@acme/tool\"").unwrap(),
            name
        );
        assert!(serde_json::from_str::<PackageName>("\"../../x\"").is_err());
    }

    fn requirement(text: &str) -> String {
        match VersionRequirement::parse(text) {
            Ok(VersionRequirement::Latest) => "latest".to_string(),
            Ok(VersionRequirement::Exact(v)) => format!("exact {v}"),
            Ok(VersionRequirement::Range(r)) => format!("range {r}"),
            Err(_) => "refused".to_string(),
        }
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "a version requirement is latest, one version or a SemVer requirement"
    )]
    fn a_requirement_is_latest_exact_or_a_range() {
        let cases = [
            ("latest", "latest"),
            ("*", "latest"),
            ("1.2.0", "exact 1.2.0"),
            ("2.0.0-rc.1", "exact 2.0.0-rc.1"),
            ("1.0.0+b", "exact 1.0.0+b"),
            ("^1.2", "range ^1.2"),
            ("~1", "range ~1"),
            (">=1, <2", "range >=1, <2"),
            ("1.x", "range 1.*"),
            ("1.2", "range ^1.2"),
            ("1", "range ^1"),
            ("", "refused"),
            ("/bar", "refused"),
            ("1.0.0/x", "refused"),
            ("1.0.0?x=1", "refused"),
            ("^bogus", "refused"),
        ];
        for (text, want) in cases {
            assert_eq!(requirement(text), want, "{text:?}");
        }
        let error = VersionRequirement::parse("^bogus").unwrap_err();
        assert!(error.to_string().contains("'^bogus'"), "{error}");
    }

    fn versions(texts: &[&str]) -> Vec<Version> {
        texts.iter().map(|t| Version::parse(t).unwrap()).collect()
    }

    fn picked(requirement: &str, published: &[&str]) -> Option<String> {
        let published = versions(published);
        VersionRequirement::parse(requirement)
            .unwrap()
            .pick(&published)
            .map(Version::to_string)
    }

    #[specforge_test(
        behavior = "upgrade_wasm_extension",
        verify = "one rule picks the version a requirement asks for"
    )]
    fn pick_takes_the_highest_release_then_a_pre_release() {
        assert_eq!(
            picked("latest", &["1.4.0", "2.0.0-beta.1"]).as_deref(),
            Some("1.4.0")
        );
        assert_eq!(
            picked("*", &["1.4.0", "2.0.0-beta.1", "1.10.0"]).as_deref(),
            Some("1.10.0")
        );
        assert_eq!(
            picked("latest", &["2.0.0-beta.1", "2.0.0-beta.2"]).as_deref(),
            Some("2.0.0-beta.2")
        );
        assert_eq!(picked("latest", &[]), None);
        assert_eq!(
            picked("^1.2", &["1.1.0", "1.2.5", "1.9.0", "2.0.0"]).as_deref(),
            Some("1.9.0")
        );
        assert_eq!(
            picked("1.x", &["0.9.0", "1.9.0", "2.0.0"]).as_deref(),
            Some("1.9.0")
        );
        // A pre-release matches only a requirement naming one.
        assert_eq!(picked("^1.2", &["1.3.0-beta.1"]), None);
        assert_eq!(
            picked(">=2.0.0-beta.1", &["1.4.0", "2.0.0-beta.1"]).as_deref(),
            Some("2.0.0-beta.1")
        );
        assert_eq!(
            picked("1.2.0", &["1.1.0", "1.2.0"]).as_deref(),
            Some("1.2.0")
        );
        assert_eq!(picked("1.3.0", &["1.1.0", "1.2.0"]), None);
    }

    #[test]
    fn compatible_with_keeps_the_major_version() {
        let current = Version::parse("1.2.3").unwrap();
        let wanted = VersionRequirement::compatible_with(&current);
        assert_eq!(wanted.to_string(), "^1.2.3");
        let published = versions(&["1.2.3", "1.9.0", "2.0.0"]);
        assert_eq!(
            wanted.pick(&published).map(Version::to_string).as_deref(),
            Some("1.9.0")
        );
        let with_build = Version::parse("1.0.0+build.1").unwrap();
        assert_eq!(
            VersionRequirement::compatible_with(&with_build).to_string(),
            "^1.0.0"
        );
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "each add argument reads as one extension source"
    )]
    fn split_requirement_splits_at_the_last_inner_at() {
        let cases = [
            ("@acme/tool", ("@acme/tool", None)),
            ("@acme/tool@^1.2", ("@acme/tool", Some("^1.2"))),
            ("@acme/tool@", ("@acme/tool", Some(""))),
            ("@acme/tool@1.0.0/x", ("@acme/tool", Some("1.0.0/x"))),
            ("foo@/bar", ("foo", Some("/bar"))),
            ("tool@1.0.0", ("tool", Some("1.0.0"))),
            ("Acme@1", ("Acme", Some("1"))),
            ("@scope", ("@scope", None)),
            ("tool", ("tool", None)),
            (
                "@acme/tool@2.0.0+build.1",
                ("@acme/tool", Some("2.0.0+build.1")),
            ),
        ];
        for (text, want) in cases {
            assert_eq!(split_requirement(text), want, "{text:?}");
        }
    }

    fn reference(text: &str) -> String {
        match PackageRef::parse(text) {
            Ok(PackageRef { name, requirement }) => match requirement {
                VersionRequirement::Latest => format!("{name} latest"),
                VersionRequirement::Exact(v) => format!("{name} exact {v}"),
                VersionRequirement::Range(r) => format!("{name} range {r}"),
            },
            Err(SpecifierError::Name(e)) => format!("name {}", variant(&e)),
            Err(SpecifierError::Unscoped(_)) => "unscoped".to_string(),
            Err(SpecifierError::EmptyRequirement { .. }) => "empty requirement".to_string(),
            Err(SpecifierError::Requirement(_)) => "requirement".to_string(),
        }
    }

    #[specforge_test(
        behavior = "parse_extension_specifier",
        verify = "each add argument reads as one extension source"
    )]
    fn a_package_ref_is_a_scoped_name_and_a_requirement() {
        let cases = [
            ("@acme/tool", "@acme/tool latest"),                // I1
            ("@acme/tool@", "empty requirement"),               // I2
            ("@acme/tool@1.2.0", "@acme/tool exact 1.2.0"),     // I3
            ("@acme/tool@^1.2", "@acme/tool range ^1.2"),       // I4
            ("@acme/tool@1.x", "@acme/tool range 1.*"),         // I5
            ("@acme/tool@1.2", "@acme/tool range ^1.2"),        // I6
            ("@acme/tool@1.0.0/x", "requirement"),              // I7
            ("@acme/tool@1.0.0?x=1", "requirement"),            // I8
            ("foo@/bar", "unscoped"),                           // I9
            ("tool@1.0.0", "unscoped"),                         // I10
            ("tool", "unscoped"),                               // I11
            ("@acme/..", "name BadPart"),                       // I12
            ("@acme/aa/bb", "name TooManyParts"),               // I13
            ("@acme/a/b", "name TooManyParts"),                 // I14
            ("@a/x", "@a/x latest"),                            // I15
            ("@acme/T ool", "name BadPart"),                    // I16
            ("Acme@1", "name BadPart"),                         // I17
            ("@acme/tool@latest", "@acme/tool latest"),         // I18
            ("@acme/tool@*", "@acme/tool latest"),              // I18
            ("@acme/tool@>=1, <2", "@acme/tool range >=1, <2"), // I19
            ("@acme/tool@^bogus", "requirement"),               // I20
            ("@acme/tool@2.0.0+build.1", "@acme/tool exact 2.0.0+build.1"), // I21
            ("@scope", "name MissingPart"),                     // I22
        ];
        for (text, want) in cases {
            assert_eq!(reference(text), want, "{text:?}");
        }
        let parsed = PackageRef::parse("@acme/tool@^1.2").unwrap();
        assert_eq!(parsed.to_string(), "@acme/tool@^1.2");
        assert_eq!(
            PackageRef::parse("@acme/tool").unwrap().to_string(),
            "@acme/tool"
        );
    }

    #[specforge_test(
        behavior = "load_extension_declaration",
        verify = "a declaration's default short name is its package name's base"
    )]
    fn a_declarations_default_short_name_is_its_package_names_base() {
        for name in [
            "@specforge/product",
            "@acme/tool",
            "@a/x",
            "@specforge/cargo-test",
            "greet",
            "x.y",
        ] {
            let mut declaration = ExtensionDeclaration::default();
            declaration.handshake.name = name.to_string();
            assert_eq!(
                declaration.short(),
                PackageName::parse(name).unwrap().base(),
                "{name}"
            );
        }
    }

    #[test]
    fn a_declaration_names_a_package_when_its_name_is_one() {
        let mut declaration = ExtensionDeclaration::default();
        declaration.handshake.name = "@acme/tool".to_string();
        assert_eq!(
            PackageName::declared_by(&declaration).unwrap().as_str(),
            "@acme/tool"
        );
        declaration.handshake.name = "../../../outside1".to_string();
        assert!(PackageName::declared_by(&declaration).is_err());
    }
}
