//! What one written reference is (ADR 0117): decided once, here, against the graph's nodes and its
//! field's [`Target`]. The linker makes the edge when the decision says so and reports what the
//! decision names.

use std::collections::BTreeMap;

use specforge_common::{Diagnostic, DiagnosticData, Sym, codes, find_close_match};

use crate::field_form::Reference;
use crate::{Graph, Node, Target};

/// What one written reference is.
pub(super) enum Resolution<'g> {
    /// It names an entity its field takes (of its target kind, or any kind when the field declares
    /// none).
    Linked,
    /// It names no entity, and its field's target kind is loaded or there is none: E003.
    Unresolved { did_you_mean: Option<&'g str> },
    /// It names no entity, and no loaded extension declares its field's target kind: I004.
    /// `provider` is the extension that declares it, where one is known.
    Unavailable {
        kind: &'g str,
        provider: Option<&'g str>,
    },
}

impl Graph {
    /// The resolution of `id`, written in a field whose target is `target`; `owners` is
    /// `GraphInputs::target_owners` (an absent kind's provider).
    pub(super) fn resolve<'g>(
        &'g self,
        target: Option<Target<'g>>,
        owners: &'g BTreeMap<String, String>,
        id: &str,
    ) -> Resolution<'g> {
        if self.nodes.contains_key(&Sym::new(id)) {
            return Resolution::Linked;
        }
        match target {
            Some(Target::Absent(kind)) => Resolution::Unavailable {
                kind,
                provider: owners.get(kind).map(String::as_str),
            },
            Some(Target::Loaded(_)) | None => Resolution::Unresolved {
                did_you_mean: find_close_match(id, self.nodes.keys().map(|s| s.as_str())),
            },
        }
    }
}

impl Resolution<'_> {
    /// Whether the reference is an edge.
    pub(super) fn links(&self) -> bool {
        matches!(self, Self::Linked)
    }

    /// Its diagnostic, if any, at the reference's span, else the entity's (the one span policy).
    pub(super) fn diagnostic(
        &self,
        node: &Node,
        field: Sym,
        reference: &Reference<'_>,
    ) -> Option<Diagnostic> {
        let span = reference.span.unwrap_or(&node.source_span).clone();
        match self {
            Self::Linked => None,
            Self::Unresolved { did_you_mean } => {
                let mut diagnostic = Diagnostic::new(
                    codes::E003,
                    format!(
                        "unresolved reference '{}' in entity '{}'",
                        reference.id, node.id.raw
                    ),
                )
                .with_span(span)
                .with_data(DiagnosticData::UnresolvedReference {
                    target: reference.id.to_string(),
                    entity: node.id.raw.as_str().to_string(),
                    field: field.as_str().to_string(),
                    did_you_mean: did_you_mean.map(str::to_string),
                });
                if let Some(close) = did_you_mean {
                    diagnostic = diagnostic.with_suggestion(format!("did you mean '{close}'?"));
                }
                Some(diagnostic)
            }
            Self::Unavailable { kind, provider } => Some(
                Diagnostic::new(
                    codes::I004,
                    format!(
                        "reference '{}' in field '{}' of '{}' targets kind '{}', which no enabled extension provides",
                        reference.id, field, node.id.raw, kind
                    ),
                )
                .with_span(span)
                .with_suggestion(match provider {
                    Some(extension) => format!(
                        "enable the extension that declares '{kind}' with `specforge add {extension}`, or drop the field"
                    ),
                    None => format!(
                        "find an extension that declares '{kind}' with `specforge search {kind}`, or drop the field"
                    ),
                }),
            ),
        }
    }
}
