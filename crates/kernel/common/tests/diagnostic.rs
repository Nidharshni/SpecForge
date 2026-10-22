//! A diagnostic is built from its code: a core code's constant gives it
//! the catalogued level, a graded code takes the pass's severity, and a
//! code that arrives as text keeps the severity it came with.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

use specforge_common::{
    CustomRuleFailure, Diagnostic, DiagnosticData, Severity, SourceSpan, Sym, codes,
};
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

fn at(line: usize) -> SourceSpan {
    SourceSpan {
        file: Sym::new("a.spec"),
        start_line: line,
        start_col: 1,
        end_line: line,
        end_col: 2,
    }
}

#[specforge_test(
    behavior = "present_diagnostics_as_json",
    verify = "a diagnostic without a span takes the span of the first entity its data names that is known; one with a span keeps it"
)]
fn placed_at_subject_takes_the_first_known_subjects_span() {
    let known = |id: &str| match id {
        "b" => Some(at(2)),
        "c" => Some(at(3)),
        _ => None,
    };
    let path = ["ghost", "b", "c", "ghost"].map(String::from).to_vec();
    let cycle =
        Diagnostic::new(codes::W061, "m").with_data(DiagnosticData::ReferenceCycle { path });
    assert_eq!(cycle.placed_at_subject(known).span, Some(at(2)));

    let failure = |entity: &str| CustomRuleFailure {
        entity: entity.into(),
        error: "e".into(),
    };
    let failures =
        Diagnostic::new(codes::W148, "m").with_data(DiagnosticData::CustomRuleFailures {
            rule: "r".into(),
            function: "f".into(),
            failures: vec![failure("ghost"), failure("c")],
        });
    assert_eq!(failures.placed_at_subject(known).span, Some(at(3)));

    let spanned = Diagnostic::new(codes::W061, "m")
        .with_span(at(9))
        .with_data(DiagnosticData::Subject { entity: "b".into() });
    assert_eq!(spanned.placed_at_subject(known).span, Some(at(9)));

    let shadowed = Diagnostic::new(codes::E013, "m").with_data(DiagnosticData::ShadowedKeyword {
        keyword: "b".into(),
    });
    assert_eq!(shadowed.placed_at_subject(known).span, None);
    assert_eq!(
        Diagnostic::new(codes::W061, "m")
            .placed_at_subject(known)
            .span,
        None
    );
}
