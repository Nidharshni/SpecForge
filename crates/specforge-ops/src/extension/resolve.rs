//! Which version of a registry package a requirement asks for (ADR 0036).
//!
//! The `Registry` port lists a package's versions and fetches one; the rule
//! that chooses among them is [`VersionRequirement::pick`], run here for
//! `add` and `update`, so production and every test fake resolve alike.

use crate::OpError;
use crate::registry::Registry;
use specforge_common::codes;
use specforge_package::PackageRef;
use specforge_package::{PackageName, Version, VersionRequirement};

/// The version `package` asks for. An exact version asks nothing of the
/// registry; anything else lists the package's versions and picks.
///
/// R-RES-002 when the package publishes none, R-RES-004 (naming the
/// requirement and the published versions) when none qualifies.
pub fn resolve(registry: &dyn Registry, package: &PackageRef) -> Result<Version, OpError> {
    resolve_requirement(registry, &package.name, &package.requirement)
}

/// [`resolve`] for a name and a requirement held apart (`update` asks for
/// `^current`, or the latest).
pub fn resolve_requirement(
    registry: &dyn Registry,
    name: &PackageName,
    requirement: &VersionRequirement,
) -> Result<Version, OpError> {
    if let Some(exact) = requirement.exact() {
        return Ok(exact.clone());
    }
    let published = registry.versions(name)?;
    if published.is_empty() {
        return Err(OpError::diagnostic(
            codes::R_RES_002,
            format!("no versions published for '{name}'"),
        ));
    }
    requirement.pick(&published).cloned().ok_or_else(|| {
        let available: Vec<String> = published.iter().map(Version::to_string).collect();
        OpError::diagnostic(
            codes::R_RES_004,
            format!(
                "no version of '{name}' satisfies '{requirement}'. Available: {}",
                available.join(", ")
            ),
        )
        .with_suggestion("try a different version requirement or check available versions")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::testing::{MemoryRegistry, Published, declaration};
    use specforge_test_macros::test as specforge_test;

    /// A registry publishing `@acme/tool` at `versions`.
    fn publishing(versions: &[&str]) -> MemoryRegistry {
        versions
            .iter()
            .fold(MemoryRegistry::new(), |registry, version| {
                registry.serving(Published::new(
                    b"\0asm".to_vec(),
                    declaration("@acme/tool", version, &[]),
                ))
            })
    }

    #[specforge_test(
        behavior = "upgrade_wasm_extension",
        verify = "one rule picks the version a requirement asks for"
    )]
    fn resolve_asks_nothing_for_an_exact_version() {
        let registry = publishing(&["1.0.0"]);

        let version = resolve(&registry, &PackageRef::parse("@acme/tool@2.5.0").unwrap()).unwrap();

        assert_eq!(version.to_string(), "2.5.0");
        assert!(registry.listed().is_empty());
    }

    #[specforge_test(
        behavior = "upgrade_wasm_extension",
        verify = "one rule picks the version a requirement asks for"
    )]
    fn resolve_picks_from_the_published_versions() {
        let registry = publishing(&["1.4.0", "1.9.0", "2.0.0-beta.1", "2.0.0"]);
        for (reference, want) in [
            ("@acme/tool", "2.0.0"),
            ("@acme/tool@1.x", "1.9.0"),
            ("@acme/tool@^1.2", "1.9.0"),
            ("@acme/tool@1.4", "1.9.0"),
        ] {
            let version = resolve(&registry, &PackageRef::parse(reference).unwrap()).unwrap();
            assert_eq!(version.to_string(), want, "{reference}");
        }
    }

    #[specforge_test(
        behavior = "upgrade_wasm_extension",
        verify = "one rule picks the version a requirement asks for"
    )]
    fn resolve_reports_r_res_004_with_the_published_versions() {
        let registry = publishing(&["1.4.0", "1.9.0"]);

        let error = resolve(&registry, &PackageRef::parse("@acme/tool@^3.0").unwrap()).unwrap_err();

        assert_eq!(error.code, "R-RES-004");
        assert!(error.message.contains("'@acme/tool'"), "{error:?}");
        assert!(error.message.contains("^3.0"), "{error:?}");
        assert!(error.message.contains("1.4.0, 1.9.0"), "{error:?}");
    }

    #[test]
    fn a_package_publishing_nothing_is_r_res_002() {
        let registry = MemoryRegistry::new().listing_none("@acme/tool");

        let error = resolve(&registry, &PackageRef::parse("@acme/tool").unwrap()).unwrap_err();

        assert_eq!(error.code, "R-RES-002");
    }
}
