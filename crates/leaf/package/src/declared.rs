//! A declaration read as a registry package (ADR 0036, 0179): what `publish` uploads.

use std::fmt;

use semver::Version;
use specforge_protocol_types::ExtensionDeclaration;

use crate::name::{PackageName, PackageNameError, SpecifierError};

/// An extension declaration whose name is a scoped package name and whose version is a full SemVer
/// version: parsed once, here, and carried with the declaration they were read from. So the name a
/// publish is routed by, the name and version it is addressed and signed by, and the manifest it
/// uploads are one package's.
#[derive(Debug, Clone)]
pub struct DeclaredPackage<'a> {
    name: PackageName,
    version: Version,
    declaration: &'a ExtensionDeclaration,
}

impl<'a> DeclaredPackage<'a> {
    /// `declaration` as a registry package. The name is checked first: no package name
    /// ([`NotAPackage::Name`]), then no scope ([`NotAPackage::Unscoped`]); then the version
    /// ([`NotAPackage::Version`]).
    pub fn of(declaration: &'a ExtensionDeclaration) -> Result<Self, NotAPackage> {
        let name = PackageName::declared_by(declaration).map_err(NotAPackage::Name)?;
        if name.scope().is_none() {
            return Err(NotAPackage::Unscoped(name));
        }
        let version =
            Version::parse(declaration.version()).map_err(|why| NotAPackage::Version {
                text: declaration.version().to_string(),
                why: why.to_string(),
            })?;
        Ok(DeclaredPackage {
            name,
            version,
            declaration,
        })
    }

    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    /// The declaration the name and version were read from: the package's manifest (ADR 0012).
    pub fn declaration(&self) -> &'a ExtensionDeclaration {
        self.declaration
    }
}

/// Why a declaration is no registry package. `Display` is the sentence E072 carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAPackage {
    /// Its name is no package name.
    Name(PackageNameError),
    /// Its name has no scope: a registry holds only `@scope/name`.
    Unscoped(PackageName),
    /// Its version is not a full SemVer version.
    Version { text: String, why: String },
}

impl fmt::Display for NotAPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotAPackage::Name(error) => error.fmt(f),
            // One wording for "a registry package is scoped": `add`'s (`SpecifierError::Unscoped`).
            NotAPackage::Unscoped(name) => SpecifierError::Unscoped(name.clone()).fmt(f),
            NotAPackage::Version { text, why } => {
                write!(f, "'{text}' is not a SemVer version: {why}")
            }
        }
    }
}

impl std::error::Error for NotAPackage {}

#[cfg(test)]
mod tests;
