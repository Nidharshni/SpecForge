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
    assert_eq!(
        (tool.scope().map(|s| s.to_string()), tool.base()),
        (Some("@acme".to_string()), "tool")
    );
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

#[specforge_test(
    behavior = "upgrade_wasm_extension",
    verify = "one rule picks the version a requirement asks for"
)]
fn pick_from_chooses_the_record_pick_chooses() {
    let version = |r: &(&str, &str)| Version::parse(r.1).ok();
    let records = [("a", "1.4.0"), ("b", "2.0.0-beta.1"), ("c", "nope")];
    assert_eq!(
        VersionRequirement::Latest.pick_from(records, version),
        Some(("a", "1.4.0"))
    );
    let none: [(&str, &str); 0] = [];
    assert_eq!(VersionRequirement::Latest.pick_from(none, version), None);
    // Of two records with the chosen version, the first.
    let twins = [("first", "1.0.0"), ("second", "1.0.0"), ("old", "0.9.0")];
    assert_eq!(
        VersionRequirement::Latest.pick_from(twins, version),
        Some(("first", "1.0.0"))
    );
}

fn highest(published: &[&str], requirements: &[&str]) -> Option<String> {
    let published = versions(published);
    let requirements: Vec<VersionReq> = requirements
        .iter()
        .map(|r| VersionReq::parse(r).unwrap())
        .collect();
    highest_matching(&published, &requirements).map(Version::to_string)
}

#[specforge_test(
    behavior = "upgrade_wasm_extension",
    verify = "one rule picks the version a requirement asks for"
)]
fn highest_matching_is_the_highest_every_requirement_matches() {
    let published = ["1.0.0", "1.5.0", "2.0.0", "2.5.0", "3.0.0"];
    assert_eq!(
        highest(&published, &[">=1.5", "<2.5"]).as_deref(),
        Some("2.0.0")
    );
    assert_eq!(highest(&published, &["^1", "^3"]), None);
    assert_eq!(
        highest(&["2.0.0-beta.1", "1.9.0"], &[">=1"]).as_deref(),
        Some("1.9.0")
    );
    assert_eq!(
        highest(&["2.0.0-beta.1", "1.9.0"], &[">=2.0.0-beta.1"]).as_deref(),
        Some("2.0.0-beta.1")
    );
    // Each requirement reads the pre-release rule on its own: the second names no pre-release.
    assert_eq!(
        highest(&["2.0.0-beta.1", "1.9.0"], &[">=2.0.0-beta.1", ">=1"]),
        None
    );
    // No requirement: the highest published.
    assert_eq!(highest(&published, &[]).as_deref(), Some("3.0.0"));
    assert_eq!(highest(&[], &[]), None);
    // Borrowed requirements read the same.
    let published = versions(&published);
    let caret = VersionReq::parse("^2").unwrap();
    assert_eq!(
        highest_matching(&published, &[&caret]).map(Version::to_string),
        Some("2.5.0".to_string())
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
