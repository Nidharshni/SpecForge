//! The diagnostic for text that must be a package name or version and is not (E072, ADR 0036).
//!
//! `specforge_package` reads names and versions and is pure; the host crates that
//! meet a refusal (ops, project, the CLI) build the diagnostic through this one function, so the
//! code and the suggestion are written once.

use crate::{Diagnostic, codes};
use std::fmt::Display;

/// E072: `why` is the refusal of the package module (its `Display` names the text), or a sentence
/// of the same shape for a version.
pub fn invalid(why: &dyn Display) -> Diagnostic {
    Diagnostic::new(codes::E072, why.to_string()).with_suggestion(
        "fix the extension's declared name or version (its SDK `name` and `version`) and rebuild, \
         or fix the specforge.json entry"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;
    use specforge_package::PackageName;

    #[test]
    fn a_refused_name_is_one_e072_naming_the_text() {
        let why = PackageName::parse("../../../outside1").unwrap_err();

        let diagnostic = invalid(&why);

        assert_eq!(diagnostic.code, "E072");
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(diagnostic.message.contains("'../../../outside1'"));
        assert!(diagnostic.suggestion.is_some());
    }
}
