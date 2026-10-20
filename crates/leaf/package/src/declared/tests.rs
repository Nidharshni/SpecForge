use super::*;
use specforge_test_macros::test as specforge_test;

fn declaring(name: &str, version: &str) -> ExtensionDeclaration {
    let mut declaration = ExtensionDeclaration::default();
    declaration.handshake.name = name.to_string();
    declaration.handshake.version = version.to_string();
    declaration
}

#[specforge_test(
    type = "DeclaredPackage",
    verify = "a declared package is a declaration whose name is a scoped package name and whose version is a full SemVer version"
)]
fn a_declared_package_is_its_declarations_scoped_name_and_version() {
    let declaration = declaring("@acme/tool", "2.0.0+build.1");
    let package = DeclaredPackage::of(&declaration).unwrap();
    assert_eq!(package.name().as_str(), "@acme/tool");
    assert_eq!(package.version().to_string(), "2.0.0+build.1");
    assert!(std::ptr::eq(package.declaration(), &declaration));
}

#[test]
fn what_is_no_registry_package_says_why_with_the_sentence_e072_carries() {
    let unscoped = declaring("greet-ext1", "0.1.0");
    let error = DeclaredPackage::of(&unscoped).unwrap_err();
    assert!(matches!(error, NotAPackage::Unscoped(_)));
    assert_eq!(
        error.to_string(),
        "'greet-ext1' is not a registry package name: registry packages are named @scope/name"
    );

    let bad_name = declaring("Greet", "0.1.0");
    let error = DeclaredPackage::of(&bad_name).unwrap_err();
    assert!(matches!(error, NotAPackage::Name(_)));
    assert!(
        error
            .to_string()
            .contains("starts with a lowercase letter or digit")
    );

    let bad_version = declaring("@acme/x", "1.0");
    let error = DeclaredPackage::of(&bad_version).unwrap_err();
    assert!(matches!(error, NotAPackage::Version { .. }));
    assert_eq!(
        error.to_string(),
        "'1.0' is not a SemVer version: unexpected end of input while parsing minor version number"
    );
}

#[test]
fn the_name_is_refused_before_the_version() {
    let error = DeclaredPackage::of(&declaring("Greet", "1.0")).unwrap_err();
    assert!(matches!(error, NotAPackage::Name(_)));
    let error = DeclaredPackage::of(&declaring("greet", "1.0")).unwrap_err();
    assert!(matches!(error, NotAPackage::Unscoped(_)));
}
