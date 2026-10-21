//! What one extension string names (ADR 0036), the reader's parse: read once, before any registry is asked.

use super::super::builtin_name;
use crate::OpError;
use specforge_common::codes;
use specforge_package::SpecifierError;
use specforge_package::{PackageRef, VersionRequirement};
use std::path::PathBuf;

/// Where an extension to add comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    /// A builtin, embedded in the binary, and the version asked for (`latest` when none): the one embedded
    /// meets it or the add is refused (R-RES-004).
    Builtin {
        name: &'static str,
        requirement: VersionRequirement,
    },
    /// A `.wasm` file on disk.
    Local(PathBuf),
    /// A registry package and the version asked for (`latest` when none).
    Registry(PackageRef),
    /// A git repository (not supported yet: E064).
    Git { url: String },
}

/// Read an `add` argument once (ADR 0036): a builtin name (with an optional `@requirement`, read as a package
/// reference's), a `git+<url>`, a
/// package reference `@scope/name[@requirement]` (it starts with `@` and does
/// not end in `.wasm`), or else a local path (a `.wasm` file or an extension's
/// crate directory, with or without `./`). A package reference that is not one
/// is E054, a requirement that is none R-RES-003, and a path that names
/// nothing is E040 when it is read ([`LocalFile::read`](super::super::candidate::LocalFile::read)). No registry is
/// asked for either.
pub(crate) fn parse(specifier: &str) -> Result<Source, OpError> {
    let specifier = specifier.trim();
    if specifier.is_empty() {
        return Err(OpError::coded(
            crate::OpErrorKind::InvalidInput,
            codes::E054,
            "empty extension specifier",
        )
        .with_suggestion(SPECIFIER_FORMS));
    }
    if let Some(builtin) = builtin_name(specifier) {
        let PackageRef { requirement, .. } = PackageRef::parse(specifier)?;
        return Ok(Source::Builtin {
            name: builtin,
            requirement,
        });
    }
    if let Some(url) = specifier.strip_prefix("git+") {
        // A `#rev` names a revision; a git source is not installable yet.
        let url = url.rfind('#').map_or(url, |hash| &url[..hash]);
        return Ok(Source::Git {
            url: url.to_string(),
        });
    }
    // Registry packages are always `@scope/name` (ADR 0036): nothing else names one.
    if specifier.ends_with(".wasm")
        || ["./", "../", "/"]
            .iter()
            .any(|prefix| specifier.starts_with(prefix))
        || !specifier.starts_with('@')
    {
        return Ok(Source::Local(PathBuf::from(specifier)));
    }
    Ok(Source::Registry(PackageRef::parse(specifier)?))
}

/// What an `add` argument may be, as the suggestion of E054.
pub(crate) const SPECIFIER_FORMS: &str =
    "use a builtin's name, './local/path.wasm', 'git+https://...', or '@scope/name[@version]'";

/// E054 for a package reference that is not one, R-RES-003 for a
/// requirement that is none.
impl From<SpecifierError> for OpError {
    fn from(error: SpecifierError) -> Self {
        match error {
            SpecifierError::Requirement(_) => OpError::coded(
                crate::OpErrorKind::InvalidInput,
                codes::R_RES_003,
                error.to_string(),
            )
            .with_suggestion(
                "use a version such as 1.2.0, a requirement such as ^1.0, ~2.3, 1.x or \
                     >=1.0.0, <2.0.0, or latest",
            ),
            _ => OpError::coded(
                crate::OpErrorKind::InvalidInput,
                codes::E054,
                format!("invalid extension specifier: {error}"),
            )
            .with_suggestion(SPECIFIER_FORMS),
        }
    }
}

/// One extension to add: its specifier as the caller gave it, and the source it reads as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Specified {
    pub(crate) specifier: String,
    pub(crate) source: Source,
}

/// A source as its specifier would spell it: the tests build a request's strings from sources.
#[cfg(test)]
impl From<Source> for String {
    fn from(source: Source) -> Self {
        source.to_string()
    }
}

/// A builtin asked at no version in particular.
#[cfg(test)]
impl Source {
    pub(crate) fn builtin(name: &'static str) -> Source {
        Source::Builtin {
            name,
            requirement: VersionRequirement::Latest,
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Builtin {
                name,
                requirement: VersionRequirement::Latest,
            } => f.write_str(name),
            Source::Builtin { name, requirement } => write!(f, "{name}@{requirement}"),
            Source::Local(path) => write!(f, "{}", path.display()),
            Source::Registry(package) => write!(f, "{package}"),
            Source::Git { url } => write!(f, "git+{url}"),
        }
    }
}

impl Source {
    /// Whether adding it installs a module under `.specforge/extensions/` and locks it.
    pub(crate) fn installs(&self) -> bool {
        matches!(self, Source::Local(_) | Source::Registry(_))
    }
}

/// E064: a git source is not installable yet.
pub(crate) fn git_not_supported(url: &str) -> OpError {
    OpError::coded(
        crate::OpErrorKind::InvalidInput,
        codes::E064,
        format!("git source '{url}' not yet supported"),
    )
}

/// E054: an add with nothing to add.
pub(crate) fn no_specifier() -> OpError {
    OpError::coded(
        crate::OpErrorKind::InvalidInput,
        codes::E054,
        "no extension specifier",
    )
    .with_suggestion(SPECIFIER_FORMS)
}

/// `error`, naming the specifier it refuses when the add has several.
pub(crate) fn named(several: bool, specifier: &str, error: OpError) -> OpError {
    if several {
        error.prefixed(format!("adding '{specifier}': "))
    } else {
        error
    }
}
