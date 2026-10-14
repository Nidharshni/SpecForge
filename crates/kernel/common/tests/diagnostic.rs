//! A diagnostic is built from its code: a core code's constant gives it
//! the catalogued level, a graded code takes the pass's severity, and a
//! code that arrives as text keeps the severity it came with.

use specforge_common::{Diagnostic, Severity, codes};
use specforge_test::prelude::*;

#[specforge_test(
    invariant = "diagnostic_code_uniqueness",
    verify = "a host diagnostic's severity is its code's catalogued level"
)]
fn diagnostic_new_takes_the_catalogued_level() {
    for (code, severity) in [
        (codes::W112, Severity::Warning),
        (codes::E003, Severity::Error),
        (codes::I202, Severity::Info),
        (codes::R001, Severity::Error),
        (codes::R003, Severity::Warning),
    ] {
        let diagnostic = Diagnostic::new(code, "m");
        assert_eq!(diagnostic.code, code.id());
        assert_eq!(diagnostic.severity, severity, "{code}");
        assert_eq!(Severity::of(code), severity, "{code}");
        assert_eq!(diagnostic.message, "m");
        assert_eq!(
            (diagnostic.span, diagnostic.suggestion, diagnostic.data),
            (None, None, None)
        );
    }
}

/// The severity of a diagnostic built from a code is the level the catalog
/// gives that code, read back from the catalog and not from the constant.
/// (A struct literal that chose another severity no longer compiles: see the
/// `compile_fail` example on [`Diagnostic`].)
#[specforge_test(
    invariant = "diagnostic_code_uniqueness",
    verify = "a host diagnostic's severity is its code's catalogued level"
)]
fn diagnostic_severity_comes_from_the_code() {
    use specforge_diagnostics::Level;
    for code in [
        codes::W112,
        codes::W150,
        codes::E003,
        codes::E069,
        codes::I202,
        codes::R001,
        codes::R_RES_005,
    ] {
        let entry = specforge_diagnostics::lookup(code.id()).expect("a constant is catalogued");
        let catalogued = match entry.level {
            Level::Error => Severity::Error,
            Level::Warning => Severity::Warning,
            Level::Info => Severity::Info,
            Level::SetByPass => unreachable!("{code} is a Code, not a GradedCode"),
        };
        assert_eq!(Diagnostic::new(code, "m").severity, catalogued, "{code}");
    }
}

#[test]
fn graded_takes_the_pass_severity() {
    for severity in [Severity::Error, Severity::Warning, Severity::Info] {
        let finding = Diagnostic::graded(codes::A010, severity, "m");
        assert_eq!(finding.code, "A010");
        assert_eq!(finding.severity, severity);
    }
}

#[test]
fn untyped_keeps_the_code_and_severity_it_is_given() {
    let diagnostic = Diagnostic::untyped("W950", Severity::Warning, "from an extension");
    assert_eq!(
        diagnostic,
        Diagnostic::untyped("W950", Severity::Warning, "from an extension"),
        "the same diagnostic the level constructors build"
    );
}

#[test]
fn is_compares_the_code() {
    let diagnostic = Diagnostic::new(codes::E003, "unresolved reference 'x'");
    assert!(diagnostic.is(codes::E003));
    assert!(!diagnostic.is(codes::E001));
    assert!(Diagnostic::untyped("E001", Severity::Info, "m").is(codes::E001));
}
