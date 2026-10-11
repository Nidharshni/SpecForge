//! What a publish names: a scoped package and a SemVer version, read from
//! the upload URL through the package module (ADR 0036).

use crate::handlers::ApiError;
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_registry_wire::code;

/// The package a publish names, read from its URL path: a scoped
/// `PackageName` (a registry holds no other) and a `Version`.
///
/// 400 `INVALID_NAME` when the segment is not a scoped package name,
/// `INVALID_VERSION` when the version is not SemVer (non-semver versions
/// poison search ordering and resolution downstream).
pub(crate) fn publish_target(
    segment: &str,
    version: &str,
) -> Result<(PackageName, Version), ApiError> {
    let name = match PackageName::from_url_segment(segment) {
        Ok(name) if name.scope().is_some() => name,
        Ok(name) => {
            return Err(ApiError::bad_request(
                code::INVALID_NAME,
                format!("'{name}' is not a scoped package name (expected @scope/name)"),
            ));
        }
        Err(why) => return Err(ApiError::bad_request(code::INVALID_NAME, why.to_string())),
    };
    let version = Version::parse(version).map_err(|_| {
        ApiError::bad_request(
            code::INVALID_VERSION,
            format!("'{version}' is not a valid SemVer version (MAJOR.MINOR.PATCH)"),
        )
    })?;
    Ok((name, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_a_scoped_name_and_a_version() {
        let (name, version) = publish_target("@acme%2Ftool", "2.0.0+build.1").unwrap();
        assert_eq!(name.as_str(), "@acme/tool");
        assert_eq!(version.to_string(), "2.0.0+build.1");
        let (name, _) = publish_target("@acme/tool", "1.0.0").unwrap();
        assert_eq!(name.scope(), Some("@acme"));
    }

    #[test]
    fn what_is_not_a_package_is_refused() {
        for name in [
            "tool",
            "@scope",
            "@a%2Fb%2Fc",
            "@acme%2F..",
            "@acme%2FT%20ool",
            "@acme%2Ftool@",
        ] {
            let error = publish_target(name, "1.0.0").err().unwrap();
            assert_eq!(error.code, "INVALID_NAME", "{name}");
        }
        for version in ["1.x", "1.2", "latest", "1.0.0/x"] {
            let error = publish_target("@acme%2Ftool", version).err().unwrap();
            assert_eq!(error.code, "INVALID_VERSION", "{version}");
        }
    }
}
