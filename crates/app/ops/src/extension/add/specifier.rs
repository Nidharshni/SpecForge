//! What an `add` argument names (ADR 0036): read once, before any registry is asked.

use super::super::builtin_name;
use crate::{OpError, Refusal};
use specforge_common::codes;
use specforge_package::PackageRef;
use specforge_package::SpecifierError;
use std::path::PathBuf;

use super::AddField;

/// Where an extension to add comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A builtin, embedded in the binary.
    Builtin(&'static str),
    /// A `.wasm` file on disk.
    Local(PathBuf),
    /// A registry package and the version asked for (`latest` when none).
    Registry(PackageRef),
    /// A git repository (not supported yet: E064).
    Git { url: String },
}

/// Read an `add` argument once (ADR 0036): a builtin name, a `git+<url>`, a
/// package reference `@scope/name[@requirement]` (it starts with `@` and does
/// not end in `.wasm`), or else a local path (a `.wasm` file or an extension's
/// crate directory, with or without `./`). A package reference that is not one
/// is E054, a requirement that is none R-RES-003, and a path that names
/// nothing is E054 when it is read ([`LocalFile::read`]). No registry is
/// asked for either.
pub fn parse(specifier: &str) -> Result<Source, OpError> {
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
        return Ok(Source::Builtin(builtin));
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
pub struct Specified {
    pub specifier: String,
    pub source: Source,
}

impl Specified {
    /// This specifier with a relative local path read from `base`: a surface with no working
    /// directory of its own (MCP) reads such a path from the project's root.
    pub fn relative_to(mut self, base: &std::path::Path) -> Self {
        if let Source::Local(path) = &mut self.source
            && path.is_relative()
        {
            *path = base.join(&*path);
        }
        self
    }
}

/// A source as its specifier would spell it.
impl From<Source> for Specified {
    fn from(source: Source) -> Self {
        Specified {
            specifier: source.to_string(),
            source,
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Builtin(name) => f.write_str(name),
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

/// Read each of `specifiers`, in order, as [`parse`] reads it. E054 when there is none; the
/// first that can't be read refuses, named when there are several. Every refusal is about the
/// specifiers.
pub fn specified<S: AsRef<str>>(specifiers: &[S]) -> Result<Vec<Specified>, Refusal<AddField>> {
    if specifiers.is_empty() {
        return Err(no_specifier().about(AddField::Extensions));
    }
    let several = specifiers.len() > 1;
    specifiers
        .iter()
        .map(|specifier| {
            let specifier = specifier.as_ref();
            parse(specifier)
                .map(|source| Specified {
                    specifier: specifier.to_string(),
                    source,
                })
                .map_err(|error| named(several, specifier, error).about(AddField::Extensions))
        })
        .collect()
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
pub(super) fn no_specifier() -> OpError {
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
