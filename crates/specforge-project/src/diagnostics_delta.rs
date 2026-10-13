//! What one update changed in what a session reports (ADR 0074).

use std::collections::HashMap;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticList};

/// The diagnostics one update added and removed, each as every surface
/// presents a diagnostic (`DiagnosticsDelta` in spec/types/graph.spec).
///
/// A diagnostic is the same before and after only when every part a surface
/// presents is: code, severity, message, span (file, start and end),
/// suggestion, data and origin (`Diagnostic`'s equality; the catalogue title
/// follows from code and origin). One that moved or changed in any part is
/// removed where it was and added where it is. Identical diagnostics count
/// one by one: removing one of two is a removal. Order is not part of it: the
/// same diagnostics listed in another order changed nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Shape)]
pub struct DiagnosticsDelta {
    /// In the order the later report lists them.
    pub added: DiagnosticList,
    /// In the order the earlier report listed them.
    pub removed: DiagnosticList,
}

impl DiagnosticsDelta {
    /// What `after` adds to `before` and what it drops, counted one by one.
    pub fn between(before: &[Diagnostic], after: &[Diagnostic]) -> Self {
        let mut unmatched: HashMap<&Diagnostic, usize> = HashMap::with_capacity(before.len());
        for diagnostic in before {
            *unmatched.entry(diagnostic).or_default() += 1;
        }
        let mut added = Vec::new();
        for diagnostic in after {
            match unmatched.get_mut(diagnostic) {
                Some(left) if *left > 0 => *left -= 1,
                _ => added.push(diagnostic.clone()),
            }
        }
        let removed: Vec<Diagnostic> = before
            .iter()
            .filter(|diagnostic| match unmatched.get_mut(*diagnostic) {
                Some(left) if *left > 0 => {
                    *left -= 1;
                    true
                }
                _ => false,
            })
            .cloned()
            .collect();
        DiagnosticsDelta {
            added: added.into(),
            removed: removed.into(),
        }
    }

    /// Nothing added, nothing removed.
    pub fn is_empty(&self) -> bool {
        self.added.0.is_empty() && self.removed.0.is_empty()
    }

    /// `other`'s additions and removals after this delta's: the delta of two
    /// reports listed one after the other, when no diagnostic is in both
    /// (MCP's I017 after the session's).
    pub fn append(&mut self, other: DiagnosticsDelta) {
        self.added.0.extend(other.added.0);
        self.removed.0.extend(other.removed.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::slice::from_ref;

    use specforge_common::{DiagnosticData, Severity, SourceSpan};
    use specforge_test_macros::test as specforge_test;

    fn at(start_line: usize, end_line: usize) -> Diagnostic {
        Diagnostic::untyped("E001", Severity::Error, "unexpected token").with_span(SourceSpan {
            file: "a.spec".into(),
            start_line,
            start_col: 1,
            end_line,
            end_col: 12,
        })
    }

    fn unresolved(did_you_mean: &str) -> DiagnosticData {
        DiagnosticData::UnresolvedReference {
            target: "alpa".into(),
            entity: "beta".into(),
            field: "behaviors".into(),
            did_you_mean: Some(did_you_mean.into()),
        }
    }

    fn lines(list: &DiagnosticList) -> Vec<(usize, usize)> {
        list.0
            .iter()
            .map(|d| {
                let span = d.span.as_ref().unwrap();
                (span.start_line, span.end_line)
            })
            .collect()
    }

    #[test]
    fn the_same_report_is_no_change() {
        let (a, b) = (at(3, 3), at(7, 7));
        assert!(DiagnosticsDelta::between(&[], &[]).is_empty());
        assert!(DiagnosticsDelta::between(from_ref(&a), from_ref(&a)).is_empty());
        // Order is not part of it: the same diagnostics listed in another
        // order changed nothing.
        assert!(DiagnosticsDelta::between(&[a.clone(), b.clone()], &[b, a]).is_empty());
    }

    #[specforge_test(
        behavior = "emit_incremental_diagnostics",
        verify = "a diagnostic is unchanged in a delta only when every presented part is"
    )]
    fn a_moved_diagnostic_is_removed_and_added() {
        let delta = DiagnosticsDelta::between(&[at(3, 3)], &[at(6, 6)]);
        assert_eq!(lines(&delta.removed), [(3, 3)]);
        assert_eq!(lines(&delta.added), [(6, 6)]);
    }

    #[specforge_test(
        behavior = "emit_incremental_diagnostics",
        verify = "a diagnostic is unchanged in a delta only when every presented part is"
    )]
    fn every_presented_part_counts() {
        let base = at(3, 3)
            .with_suggestion("remove it")
            .with_data(unresolved("alpha"));
        let mut other_origin = base.clone();
        other_origin.origin = Some("@acme/x".into());
        let mut other_severity = base.clone();
        other_severity.severity = Severity::Warning;
        let mut other_end = base.clone();
        other_end.span.as_mut().unwrap().end_line = 5;
        let mut other_message = base.clone();
        other_message.message = "another message".into();
        let mut other_code = base.clone();
        other_code.code = "E002".into();
        let variants = [
            ("span end", other_end),
            ("severity", other_severity),
            ("suggestion", base.clone().with_suggestion("rename it")),
            ("data", base.clone().with_data(unresolved("alpha2"))),
            ("origin", other_origin),
            ("message", other_message),
            ("code", other_code),
        ];
        for (part, changed) in variants {
            let delta = DiagnosticsDelta::between(from_ref(&base), from_ref(&changed));
            assert_eq!(delta.removed.0, from_ref(&base), "{part}");
            assert_eq!(delta.added.0, from_ref(&changed), "{part}");
        }
    }

    #[specforge_test(
        behavior = "emit_incremental_diagnostics",
        verify = "a diagnostic is unchanged in a delta only when every presented part is"
    )]
    fn identical_diagnostics_count_one_by_one() {
        let d = at(3, 3);
        let removed = DiagnosticsDelta::between(&[d.clone(), d.clone()], from_ref(&d));
        assert_eq!(removed.removed.0, from_ref(&d));
        assert!(removed.added.0.is_empty());
        let added = DiagnosticsDelta::between(from_ref(&d), &[d.clone(), d.clone()]);
        assert_eq!(added.added.0, from_ref(&d));
        assert!(added.removed.0.is_empty());
    }

    #[specforge_test(
        behavior = "emit_incremental_diagnostics",
        verify = "a diagnostic is unchanged in a delta only when every presented part is"
    )]
    fn the_delta_keeps_each_report_order() {
        let delta = DiagnosticsDelta::between(
            &[at(1, 1), at(2, 2), at(3, 3)],
            &[at(9, 9), at(8, 8), at(2, 2)],
        );
        assert_eq!(lines(&delta.removed), [(1, 1), (3, 3)]);
        assert_eq!(lines(&delta.added), [(9, 9), (8, 8)]);
    }

    #[test]
    fn append_lists_the_second_delta_after_the_first() {
        let mut first = DiagnosticsDelta::between(&[at(1, 1)], &[at(2, 2)]);
        first.append(DiagnosticsDelta::between(&[at(3, 3)], &[at(4, 4)]));
        assert_eq!(lines(&first.removed), [(1, 1), (3, 3)]);
        assert_eq!(lines(&first.added), [(2, 2), (4, 4)]);
    }
}
