//! A read's selection (ADR 0177): the entity a request names, the hops counted from it and the
//! kinds it keeps, checked once by the project view.

use crate::view_support::{Assembled, Viewed, registries};
use specforge_ops::read::export::{Request, export, export_recorded};

/// A project with one behavior, `login`.
fn project() -> Assembled {
    Assembled::over(registries(&["behavior", "feature"], &[]))
        .source("behavior login \"Log in\" {\n}\n")
}

fn codes(notices: &[specforge_common::Diagnostic]) -> Vec<String> {
    notices.iter().map(|n| n.code.to_string()).collect()
}

fn assert_i020_for_behaviour(notices: &[specforge_common::Diagnostic]) {
    assert_eq!(codes(notices), ["I020"], "{notices:?}");
    assert_eq!(notices[0].message, "unknown entity kind 'behaviour'");
    assert_eq!(
        notices[0].suggestion.as_deref(),
        Some("did you mean 'behavior'?")
    );
}

#[test]
#[ignore = "flipped by T5"]
fn export_reports_an_unknown_kind() {
    let project = project();
    let view = project.view();
    for scope in [None, Some("login")] {
        let request = Request {
            kinds: vec!["behaviour"],
            scope,
            ..Request::default()
        };
        let exported = export(&view, &request).unwrap();
        assert_i020_for_behaviour(exported.notices());
        let recorded = export_recorded(&view, &request);
        assert_i020_for_behaviour(recorded.notices());
    }
}
