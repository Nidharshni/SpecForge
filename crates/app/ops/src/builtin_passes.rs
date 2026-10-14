//! The analysis passes the host runs itself under `specforge analyze`.
//! Every other pass is an extension's (`specforge_project::passes`);
//! coverage is `@specforge/testing`'s (ADR 0002).

use std::collections::HashMap;

use specforge_common::{Diagnostic, Severity, codes};
use specforge_parser::FieldValue;
use specforge_project::passes::{AnalysisContext, Finding};

/// Result of one analysis pass.
pub struct PassReport {
    pub name: &'static str,
    pub description: &'static str,
    pub findings: Vec<Finding>,
    pub summary: serde_json::Value,
}

/// Built-in passes. Coverage is owned by `@specforge/testing`
/// (`@specforge/testing:coverage`, ADR 0002).
pub const PASS_NAMES: &[&str] = &["contracts"];

/// The extension pass that owns proof coverage (and the `--min` gate).
pub const COVERAGE_PASS: &str = "@specforge/testing:coverage";

// ── contracts ───────────────────────────────────────────────────────────────

/// `contracts` — requires/ensures/maintains contract coverage (RES-25).
///
/// A contract-bearing kind (any kind with registered reference fields such as
/// `requires`/`ensures`/`maintains`) whose entities declare none of them is
/// reported as unconstrained. Reference existence is already checked by the
/// compiler (E003) and is not repeated here.
pub fn pass_contracts(ctx: &AnalysisContext) -> (Vec<Finding>, serde_json::Value) {
    let mut contract_fields: HashMap<&str, Vec<&str>> = HashMap::new();
    for (kind, field, entry) in ctx.field_registry.iter() {
        if !entry.field_type().is_reference() {
            continue;
        }
        // A reference to a contract kind (an extension declares which:
        // `contract_target`, e.g. invariants and formal properties) is a
        // contract obligation.
        if entry
            .declared()
            .target_kind
            .as_deref()
            .and_then(|t| ctx.kind_registry.get(t))
            .is_some_and(|kind| kind.declared.contract_target)
        {
            contract_fields.entry(kind).or_default().push(field);
        }
    }
    for fields in contract_fields.values_mut() {
        // Suggestions render this list — keep it registry-order-independent
        // (hardening-plan D4 / R-6).
        fields.sort_unstable();
    }

    let mut findings = Vec::new();
    let mut contract_entities = 0usize;
    let mut unconstrained = 0usize;
    let mut obligation_refs = 0usize;

    // Sorted node order: findings must not depend on HashMap seeding (R-6).
    let mut nodes: Vec<_> = ctx.graph.nodes();
    nodes.sort_by_key(|n| n.id.raw);
    for node in nodes {
        let Some(fields) = contract_fields.get(node.kind.raw.as_str()) else {
            continue;
        };
        let declared = fields
            .iter()
            .filter(|f| {
                matches!(
                    node.fields.get(f),
                    Some(FieldValue::ReferenceList(items)) if !items.is_empty()
                )
            })
            .count();
        if declared > 0 {
            contract_entities += 1;
            obligation_refs += declared;
        } else {
            unconstrained += 1;
            findings.push(
                Diagnostic::graded(
                    codes::A010,
                    Severity::Info,
                    format!(
                        "{} '{}' declares no contract obligations",
                        node.kind.raw, node.id.raw
                    ),
                )
                .with_span(node.source_span.clone())
                .with_suggestion(format!(
                    "add requires/ensures/maintains references (registered for '{}': {})",
                    node.kind.raw,
                    fields.join(", ")
                )),
            );
        }
    }

    let summary = serde_json::json!({
        "contract_entities": contract_entities,
        "unconstrained_entities": unconstrained,
        "obligation_references": obligation_refs,
    });
    (findings, summary)
}

/// Run one named built-in pass. Returns `None` for unknown pass names.
pub fn run_pass(ctx: &AnalysisContext, pass: &str) -> Option<PassReport> {
    let (name, description, findings, summary) = match pass {
        "contracts" => {
            let (findings, summary) = pass_contracts(ctx);
            (
                "contracts",
                "entities of contract-bearing kinds without requires/ensures/maintains obligations",
                findings,
                summary,
            )
        }
        _ => return None,
    };
    Some(PassReport {
        name,
        description,
        findings,
        summary,
    })
}
