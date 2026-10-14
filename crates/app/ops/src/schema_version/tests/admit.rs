use super::*;

// ===========================================================================
// Slice 6: Version Negotiation (a reader's version is admitted, never applied)
// ===========================================================================

/// The sample schema versioned as `major.minor.patch`.
fn versioned_at(major: u32, minor: u32, patch: u32) -> VersionedSchema {
    let mut schema = sample_schema();
    schema.schema_version = SchemaVersion::new(major, minor, patch);
    VersionedSchema::new(schema, Since::NoRecord)
}

fn parsed(text: &str) -> SchemaVersion {
    text.parse().unwrap()
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of the same major, not newer, is admitted and the export carries the current version"
)]
fn a_reader_of_an_earlier_minor_is_admitted() {
    let versioned = versioned_at(1, 3, 2);

    for requested in ["1.0.0", "1.2.9", "1.3.0", "1.3.1"] {
        assert_eq!(versioned.admits(&parsed(requested)), Ok(()), "{requested}");
    }
    // Admitting changes nothing: the schema keeps its own version.
    assert_eq!(versioned.version(), &SchemaVersion::new(1, 3, 2));
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of the same major, not newer, is admitted and the export carries the current version"
)]
fn the_current_version_is_admitted() {
    let versioned = versioned_at(1, 3, 2);
    assert_eq!(versioned.admits(&parsed("1.3.2")), Ok(()));
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of another major, or a newer one, is refused as incompatible_schema_version naming the readable range"
)]
fn another_major_is_not_admitted() {
    let versioned = versioned_at(1, 3, 0);

    for requested in ["2.0.0", "0.9.0"] {
        let refusal = versioned.admits(&parsed(requested)).unwrap_err();
        assert_eq!(refusal.requested, parsed(requested));
        assert_eq!(refusal.current, SchemaVersion::new(1, 3, 0));
        assert!(
            refusal
                .to_string()
                .contains("has another major version: this project's schema is 1.3.0"),
            "{refusal}"
        );
    }
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of another major, or a newer one, is refused as incompatible_schema_version naming the readable range"
)]
fn a_newer_version_is_not_admitted() {
    let versioned = versioned_at(1, 3, 0);

    for requested in ["1.3.1", "1.4.0", "1.99.0"] {
        assert!(versioned.admits(&parsed(requested)).is_err(), "{requested}");
    }
}

#[test]
fn a_pre_release_of_the_current_version_is_admitted() {
    let versioned = versioned_at(1, 3, 0);

    assert_eq!(versioned.admits(&parsed("1.3.0-beta")), Ok(()));
    // A pre-release label sorts below its release: 1.3.1-beta is newer than 1.3.0.
    assert!(versioned.admits(&parsed("1.3.1-beta")).is_err());
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of another major, or a newer one, is refused as incompatible_schema_version naming the readable range"
)]
fn the_refusal_names_the_readable_range() {
    let versioned = versioned_at(1, 3, 0);

    assert_eq!(
        versioned.admits(&parsed("2.0.0")).unwrap_err().to_string(),
        "requested schema version 2.0.0 has another major version: this project's schema is 1.3.0, which readers of 1.0.0 to 1.3.0 read"
    );
    assert_eq!(
        versioned.admits(&parsed("1.6.0")).unwrap_err().to_string(),
        "requested schema version 1.6.0 is newer than this project's schema 1.3.0, which readers of 1.0.0 to 1.3.0 read"
    );
    assert_eq!(NotAdmitted::CODE, "incompatible_schema_version");
}
