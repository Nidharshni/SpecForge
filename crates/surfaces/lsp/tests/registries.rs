//! The registries an LSP test reads, as an extension declares them: with the
//! SDK builders an extension author uses, turned into registries by the
//! registry build (`build_registries`) that production runs over every loaded
//! declaration. A test never writes a registry entry, so a change to the
//! registry's entry types changes no test (ADR 0025).

use specforge_extension_sdk::ExtensionDeclaration;
use specforge_extension_sdk::prelude::*;
use specforge_registry::RegistryBuild;

/// The declaration `extension` makes with `declare`.
pub fn declaration(
    extension: &str,
    declare: impl FnOnce(&mut ContributionsBuilder),
) -> ExtensionDeclaration {
    let mut c = ContributionsBuilder::new(ExtensionMeta::new(extension, "1.0.0"));
    declare(&mut c);
    c.declaration()
}

/// The registries `extension` declares with the SDK, as the registry build
/// makes them from its declaration.
pub fn registries(
    extension: &str,
    declare: impl FnOnce(&mut ContributionsBuilder),
) -> RegistryBuild {
    registries_of(vec![declaration(extension, declare)])
}

/// The registries of several extensions, in load order.
pub fn registries_of(declarations: Vec<ExtensionDeclaration>) -> RegistryBuild {
    specforge_registry::build_registries(declarations)
}

/// A kind `name`: testable (and so accepting `verify`) or not.
pub fn kind(c: &mut ContributionsBuilder, name: &str, testable: bool) {
    c.kind(name, |k| {
        k.testable(testable).supports_verify(testable);
    });
}

/// Rule W004: entities of `kind` owe obligations, so one that declares no
/// `verify` is reported and counts toward coverage.
pub fn obligating(c: &mut ContributionsBuilder, kind: &str) {
    c.rule("W004", |r| {
        r.check(CheckKind::NoVerifyStatements)
            .target_kind(kind)
            .field("verify")
            .severity(ValidationSeverity::Warning)
            .message_template("{kind} '{id}' is testable but declares no verify obligations");
    });
}
