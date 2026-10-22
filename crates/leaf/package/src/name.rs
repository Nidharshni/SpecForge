//! What an extension package is called and which of its versions is asked for (ADR 0036).
//!
//! A string enters as one of these types where it enters (the `add` argument, a `specforge.json`
//! entry, a lock entry, a declaration being installed or published, a registry URL) and is passed
//! as the type from then on. No other code splits `name@version` or decides what a name may hold.
//!
//! The module is pure: no I/O and no diagnostics. Callers map its errors to their codes.

use std::borrow::Borrow;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use specforge_protocol_types::ExtensionDeclaration;

use crate::scope::Scope;

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
    /// The name `declaration` declares, read as a package name: `add`, `publish` and the extension
    /// load's admit of a `.wasm` file entry require one (ADR 0036, 0223).
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

    /// The scope of a scoped name (`@acme`); `None` for a local name.
    pub fn scope(&self) -> Option<Scope> {
        self.slash
            .map(|slash| Scope(self.text[..slash].to_string()))
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
            Some(scope) => PathBuf::from(scope.as_str()).join(self.base()),
            None => PathBuf::from(&self.text),
        }
    }
}

/// One part of a name: `a-z 0-9 . _ -`, starting with `a-z` or `0-9`.
pub(crate) fn is_part(part: &str) -> bool {
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

    /// The version this asks for without listing the package: `Some` for `Exact`.
    pub fn exact(&self) -> Option<&Version> {
        match self {
            VersionRequirement::Exact(version) => Some(version),
            _ => None,
        }
    }

    /// The best of `published` this asks for, `None` when none qualifies. The one resolution
    /// rule: ops `add`, `upgrade_target` and every registry's search call it.
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
            VersionRequirement::Range(requirement) => {
                highest_matching(published, std::slice::from_ref(requirement))
            }
        }
    }

    /// The one of `items` whose version [`pick`](Self::pick) chooses, for callers that hold records
    /// rather than versions (a registry's search lists a package's record at the version `latest`
    /// picks). An item whose `version` is `None` (text that is no SemVer version) is never chosen;
    /// of several items carrying the chosen version, the first.
    pub fn pick_from<T>(
        &self,
        items: impl IntoIterator<Item = T>,
        version: impl Fn(&T) -> Option<Version>,
    ) -> Option<T> {
        let items: Vec<(Version, T)> = items
            .into_iter()
            .filter_map(|item| version(&item).map(|v| (v, item)))
            .collect();
        let versions: Vec<Version> = items.iter().map(|(v, _)| v.clone()).collect();
        let picked = self.pick(&versions)?.clone();
        items
            .into_iter()
            .find(|(v, _)| *v == picked)
            .map(|(_, item)| item)
    }
}

/// The highest of `published` that every one of `requirements` matches, each read as semver reads a
/// requirement (a pre-release matches one only when it names a pre-release of the same
/// `MAJOR.MINOR.PATCH`); `None` when none does, or when `requirements` is empty and `published` is.
/// [`VersionRequirement::pick`] reads a range through it, [`upgrade_target`](crate::upgrade_target) the
/// range an update admits, and a version diamond's unification every requirer's peer range
/// (`PeerRequirement::range`).
pub fn highest_matching<'a, R: Borrow<VersionReq>>(
    published: &'a [Version],
    requirements: &[R],
) -> Option<&'a Version> {
    published
        .iter()
        .filter(|version| requirements.iter().all(|r| r.borrow().matches(version)))
        .max()
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
mod tests;
