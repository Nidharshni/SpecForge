//! Which version of a registry package an operation asks for (ADR 0036): `add` resolves a
//! requirement ([`VersionRequirement::pick`]), `update` an upgrade target
//! ([`upgrade_target`]); the `Registry` port lists and fetches, never chooses.

use crate::OpError;
use crate::registry::Registry;
use specforge_common::codes;
#[cfg(doc)]
use specforge_package::VersionRequirement;
use specforge_package::{LockedVersion, PackageName, PackageRef, Reach, Version, upgrade_target};

/// The version `package` asks for. An exact version asks nothing of the
/// registry; anything else lists the package's versions and picks.
///
/// R-RES-002 when the package publishes none, R-RES-004 (naming the
/// requirement and the published versions) when none qualifies.
pub fn resolve(registry: &dyn Registry, package: &PackageRef) -> Result<Version, OpError> {
    let PackageRef { name, requirement } = package;
    if let Some(exact) = requirement.exact() {
        return Ok(exact.clone());
    }
    let published = published(registry, name)?;
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

/// The version `name`, locked at `locked`, is updated to within `reach`, or `None` when it is up
/// to date ([`upgrade_target`]: never the locked version or below).
///
/// The registry's errors pass through (R-RES-001 for a package it does not hold); R-RES-002 when
/// it publishes no version of it.
pub fn upgrade(
    registry: &dyn Registry,
    name: &PackageName,
    locked: &LockedVersion,
    reach: Reach,
) -> Result<Option<Version>, OpError> {
    let published = published(registry, name)?;
    Ok(upgrade_target(locked, &published, reach).cloned())
}

/// The versions `registry` publishes of `name`; R-RES-002 when it lists none.
fn published(registry: &dyn Registry, name: &PackageName) -> Result<Vec<Version>, OpError> {
    let published = registry.versions(name)?;
    if published.is_empty() {
        return Err(OpError::diagnostic(
            codes::R_RES_002,
            format!("no versions published for '{name}'"),
        ));
    }
    Ok(published)
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

    #[test]
    fn upgrade_lists_once_and_answers_none_when_up_to_date() {
        let registry = publishing(&["1.4.0", "2.0.0-beta.1"]);
        let name = PackageName::parse("@acme/tool").unwrap();
        let locked = LockedVersion::SemVer(Version::parse("2.0.0-beta.1").unwrap());

        let target = upgrade(&registry, &name, &locked, Reach::Major).unwrap();

        assert_eq!(target, None);
        assert_eq!(registry.listed().len(), 1);
    }

    #[test]
    fn upgrade_of_a_package_publishing_nothing_is_r_res_002() {
        let registry = MemoryRegistry::new().listing_none("@acme/tool");
        let name = PackageName::parse("@acme/tool").unwrap();

        let error = upgrade(
            &registry,
            &name,
            &LockedVersion::read("1.0.0"),
            Reach::Compatible,
        )
        .unwrap_err();

        assert_eq!(error.code, "R-RES-002");
    }
}
