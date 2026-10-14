use specforge_common::{Diagnostic, codes};

/// W017: a kind declared `testable` that does not accept `verify`
/// statements, so its entities could never declare the obligations
/// coverage counts. [`super::build::build_registries`] runs it once the
/// kinds are populated. A kind that accepts `verify` but is not testable
/// (a formal `property`) is a deliberate combination, not reported.
pub(crate) fn validate_extension_testability(kind_reg: &crate::KindRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (_, entry) in kind_reg.iter() {
        if entry.testable && !entry.supports_verify {
            diagnostics.push(
                Diagnostic::new(
                    codes::W017,
                    format!(
                        "entity kind '{}' from '{}' is testable but does not support verify statements",
                        entry.kind_name, entry.source_extension
                    ),
                )
                .with_suggestion(
                    "declare the kind with supports_verify (KindBuilder::supports_verify)"
                        .to_string(),
                ),
            );
        }
    }

    // Sort for deterministic output
    diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
    diagnostics
}
