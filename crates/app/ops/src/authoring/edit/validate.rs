//! Validating a plan (ADR 0151): the edited project is compiled from the planned texts; an error it reports that the
//! project's checked report (`CompiledProject::checked_diagnostics`: every check run, even after an update that
//! skipped them) does not have refuses the plan (E080). Diagnostics are matched without their positions, since an inserted
//! block moves every later diagnostic of its file.

use super::EditField;
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
use std::collections::{BTreeMap, HashMap};

type Key = (String, Severity, String, String);

fn key(d: &Diagnostic) -> Key {
    (
        d.code.clone(),
        d.severity,
        d.message.clone(),
        d.span.as_ref().map_or("", |s| s.file.as_str()).to_string(),
    )
}

/// The diagnostics of `after` that `before` does not have, one by one: (errors, warnings and infos).
pub(crate) fn added(
    before: &[Diagnostic],
    after: Vec<Diagnostic>,
) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut had: HashMap<Key, usize> = HashMap::new();
    for d in before {
        *had.entry(key(d)).or_default() += 1;
    }
    let (mut errors, mut rest) = (Vec::new(), Vec::new());
    for d in after {
        match had.get_mut(&key(&d)) {
            Some(n) if *n > 0 => *n -= 1,
            _ if d.severity == Severity::Error => errors.push(d),
            _ => rest.push(d),
        }
    }
    (errors, rest)
}

/// What the project reports once `texts` are written that its checked report does not have now: the warnings and infos, or E080
/// (`CompilationFailed`, about no field) naming each new error.
pub fn check(
    view: &ProjectView<'_>,
    texts: &BTreeMap<String, String>,
) -> Result<Vec<Diagnostic>, Refusal<EditField>> {
    let before = view.project().checked_diagnostics();
    let (errors, rest) = added(&before, view.project().edited(texts).diagnostics());
    if errors.is_empty() {
        return Ok(rest);
    }
    let mut message = format!(
        "the edit would leave the project with {} error{} it does not have; nothing was written",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" }
    );
    for error in &errors {
        message.push_str(&format!("\n  error[{}]: {}", error.code, error.message));
    }
    let data = serde_json::json!({ "diagnostics": DiagnosticList(errors) });
    Err(
        OpError::coded(OpErrorKind::CompilationFailed, codes::E080, message)
            .with_data(data)
            .into(),
    )
}
