//! Validating a plan (ADR 0151): the edited project is compiled from the planned texts; an error it reports that the
//! project's checked report (`CompiledProject::checked_diagnostics`: every check run, even after an update that
//! skipped them) does not have refuses the plan (E080), and so does a graph change its edits do not declare (ADR 0198). Diagnostics are matched without their positions: neither the
//! span nor the digits of a message are in the key, since a message may name a position (E002, E078) and an inserted
//! block moves every later one.

use super::EditField;
use super::declared::Declared;
use crate::view::ProjectView;
use crate::{OpError, OpErrorKind, Refusal};
use specforge_common::{Diagnostic, DiagnosticList, Severity, codes};
use specforge_graph::{GraphDelta, compute_graph_delta_over};
use std::collections::{BTreeMap, HashMap};

type Key = (String, Severity, String, String);

/// The message with each run of ASCII digits collapsed, so a line that moved is the same diagnostic.
fn shape(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for c in message.chars() {
        if !c.is_ascii_digit() || !out.ends_with('#') {
            out.push(if c.is_ascii_digit() { '#' } else { c });
        }
    }
    out
}

fn key(d: &Diagnostic) -> Key {
    (
        d.code.clone(),
        d.severity,
        shape(&d.message),
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

/// What the project reports once `texts` are written that its checked report does not have now: the warnings and
/// infos; or E080 naming each new error; or, when the edited graph changes what `declared` does not name,
/// `undeclared_change` (ADR 0198) naming the remainder.
pub fn check(
    view: &ProjectView<'_>,
    texts: &BTreeMap<String, String>,
    declared: &Declared,
) -> Result<Vec<Diagnostic>, Refusal<EditField>> {
    let before = view.project().checked_diagnostics();
    let post = view.project().edited(texts);
    let (errors, rest) = added(&before, post.diagnostics());
    if !errors.is_empty() {
        return Err(new_errors(errors));
    }
    let delta = compute_graph_delta_over(
        view.project().graph(),
        post.graph(),
        texts.keys().map(String::as_str),
    );
    let beyond = declared.beyond(delta);
    if beyond.is_empty() {
        return Ok(rest);
    }
    Err(undeclared(&beyond))
}

fn new_errors(errors: Vec<Diagnostic>) -> Refusal<EditField> {
    let mut message = format!(
        "the edit would leave the project with {} error{} it does not have; nothing was written",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" }
    );
    for error in &errors {
        message.push_str(&format!("\n  error[{}]: {}", error.code, error.message));
    }
    let data = serde_json::json!({ "diagnostics": DiagnosticList(errors) });
    OpError::coded(OpErrorKind::CompilationFailed, codes::E080, message)
        .with_data(data)
        .into()
}

/// The change a typed edit planned and did not declare: a defect of the edit, refused before anything is written.
pub const UNDECLARED_CHANGE: &str = "undeclared_change";

fn undeclared(beyond: &GraphDelta) -> Refusal<EditField> {
    let mut message = String::from(
        "the edit would change what it does not ask for; nothing was written (a defect of the typed edit: please report it)",
    );
    for n in beyond.added_nodes.iter().chain(&beyond.removed_nodes) {
        message.push_str(&format!("\n  entity '{}': added or removed", n.id));
    }
    for n in &beyond.modified_nodes {
        let what: Vec<String> = n
            .changed_fields
            .iter()
            .map(|f| format!("field {f}"))
            .chain(
                n.changed_aspects
                    .iter()
                    .map(|a| format!("{a:?}").to_lowercase()),
            )
            .collect();
        message.push_str(&format!("\n  entity '{}': {}", n.id, what.join(", ")));
    }
    for (sign, edges) in [
        ("added", &beyond.added_edges),
        ("removed", &beyond.removed_edges),
    ] {
        for e in edges {
            message.push_str(&format!(
                "\n  edge {} -[{}]-> {} {sign}",
                e.source, e.label, e.target
            ));
        }
    }
    OpError::new(OpErrorKind::CompilationFailed, UNDECLARED_CHANGE, message)
        .with_data(serde_json::json!({ "delta": beyond }))
        .into()
}
