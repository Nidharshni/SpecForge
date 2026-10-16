//! `specforge analyze --prove` — formal expression verification.
//!
//! Rungs 2–3 of the formal ladder (RES-25; Leino/de Moura anchors):
//!
//! What the pass reads is declared, not named (ADR 0009): a field whose
//! extension gives it the **bound** proof role (a governance constraint's
//! `metric`, a formal axiom's `expression`) holds facts the solver assumes;
//! one with the **claim** role (a formal property's or invariant's
//! `expression`) holds statements that must follow from them. A field with
//! no role is not read, whatever its name.
//!
//! **Consistency** (rung 2): every bound field's parseable lines become
//! individually named conjuncts asserted corpus-wide with z3. An unsat
//! result carries the unsat core naming the minimal set of bounds that
//! contradict each other (E046), across files, with per-bound citations.
//!
//! **Entailment** (rung 3): every parseable line of a claim field is a
//! *claim*. Each claim is checked as a verification condition
//! `declared_bounds ⇒ claim` by asking z3 whether
//! `bounds ∧ ¬claim` is satisfiable: unsat proves the claim from the
//! declared bounds; sat yields a **counterexample model** — concrete
//! values satisfying every declared bound while violating the claim
//! (W139), which is exactly the failure evidence a counterexample-guided
//! loop consumes.
//!
//! Prose lines that do not parse as expressions are skipped and counted.
//! Unit suffixes are normalized to their dimension base before encoding
//! (time → milliseconds, data → bytes; unknown units compare raw), so
//! `100ms` vs `1s` compares correctly and counterexample models render
//! in the declared unit.

use specforge_common::{Diagnostic, SourceSpan, Sym, codes};
use specforge_graph::Graph;
use specforge_parser::{SpannedExpr, parse_expression};
use specforge_registry::FieldRegistry;
use specforge_registry::ProofRole;

use crate::solver::{Availability, SolveFailure, Solver};

/// Result of one prove run over the compiled project.
pub struct ProveReport {
    pub findings: Vec<Diagnostic>,
    pub summary: serde_json::Value,
    /// Ids of entities whose formal claims were ENTAILED from the declared
    /// bounds; consumed by the coverage pass discharge funnel.
    pub proved_claim_ids: Vec<String>,
}

// ── the pass ────────────────────────────────────────────────────────────────

/// One parseable formal bound, with provenance for diagnostics.
struct Conjunct {
    expr: SpannedExpr,
    text: String,
    /// Human location: field-relative line (string form) or file line
    /// (first-class `expr { }` form).
    loc: String,
    /// Owning entity's source span (attached by the collection loops).
    span: SourceSpan,
}

struct Claim {
    id: String,
    span: SourceSpan,
    expr: SpannedExpr,
    text: String,
}

/// Parse the field `name`, which may be first-class (`expr { }`) or a
/// string block (one comparison per line), returning conjuncts plus the
/// number of skipped prose lines.
fn conjuncts_from_field(name: &str, value: &specforge_graph::FieldValue) -> (Vec<Conjunct>, usize) {
    let mut skipped = 0usize;
    let mut out = Vec::new();
    match value {
        specforge_graph::FieldValue::String(text) => {
            for (i, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                match parse_expression(line) {
                    Ok(expr) => out.push(Conjunct {
                        expr,
                        text: line.trim().to_string(),
                        loc: format!("line {} of '{name}'", i + 1),
                        span: SourceSpan {
                            file: Sym::new(""),
                            start_line: 0,
                            start_col: 0,
                            end_line: 0,
                            end_col: 0,
                        },
                    }),
                    Err(_) => skipped += 1,
                }
            }
        }
        specforge_graph::FieldValue::Expression(exprs) => {
            for e in exprs {
                out.push(Conjunct {
                    expr: e.clone(),
                    text: e.to_string(),
                    loc: format!("line {}", e.span.start_line),
                    span: SourceSpan {
                        file: Sym::new(""),
                        start_line: 0,
                        start_col: 0,
                        end_line: 0,
                        end_col: 0,
                    },
                });
            }
        }
        _ => {}
    }
    (out, skipped)
}

fn note_failure(
    failure: SolveFailure,
    runtime_failure: &mut bool,
    timed_out: &mut Option<std::time::Duration>,
) {
    match failure {
        SolveFailure::TimedOut { after } => *timed_out = Some(after),
        SolveFailure::Failed => *runtime_failure = true,
    }
}

/// Run the prove pass over `solver`: consistency over declared bounds (E046
/// with unsat cores) and entailment of declared claims with counterexample
/// models (W139).
pub(crate) fn run(graph: &Graph, fields: &FieldRegistry, solver: &dyn Solver) -> ProveReport {
    let mut findings = Vec::new();
    let mut skipped_prose_lines = 0usize;
    let mut entities_with_bounds = 0usize;
    let mut conjunct_count = 0usize;
    let mut axioms: Vec<Conjunct> = Vec::new();
    let mut claims: Vec<Claim> = Vec::new();
    let (solver_version, unavailable) = match solver.availability() {
        Availability::Ready { version } => (Some(version), None),
        Availability::Unavailable(diagnostic) => (None, Some(diagnostic)),
    };
    let solver_available = solver_version.is_some();

    for node in graph.nodes() {
        let kind = node.kind.raw.as_str();
        let mut has_bounds = false;
        for entry in node.fields.entries() {
            let name = entry.key.as_str();
            // Only fields an extension declares a proof role for are read.
            let Some(role) = fields.get(kind, name).and_then(|field| field.proof_role()) else {
                continue;
            };
            let (mut conjuncts, skipped) = conjuncts_from_field(name, &entry.value);
            skipped_prose_lines += skipped;
            match role {
                // Bounds: facts the solver assumes.
                ProofRole::Bound => {
                    has_bounds |= !conjuncts.is_empty();
                    conjunct_count += conjuncts.len();
                    for c in &mut conjuncts {
                        c.span = node.source_span.clone();
                    }
                    axioms.extend(conjuncts);
                }
                // Claims: statements the bounds must entail.
                ProofRole::Claim => {
                    claims.extend(conjuncts.into_iter().map(|c| Claim {
                        id: node.id.raw.to_string(),
                        span: node.source_span.clone(),
                        expr: c.expr,
                        text: c.text,
                    }));
                }
            }
        }
        if has_bounds {
            entities_with_bounds += 1;
        }
    }

    let mut display_units: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for conj in &axioms {
        note_display_unit(&conj.expr, &mut display_units);
    }
    for claim in &claims {
        note_display_unit(&claim.expr, &mut display_units);
    }

    let mut satisfiable = false;
    let mut unsat = false;
    let mut claims_proved = 0usize;
    let mut claims_unproved = 0usize;
    let mut proved_claim_ids: Vec<String> = Vec::new();
    // Loud, deterministic behavior when the solver is missing or fails
    // mid-run (hardening-plan D3): analysis output must never silently
    // depend on machine state.
    let mut solver_runtime_failure = false;
    let mut solver_timed_out: Option<std::time::Duration> = None;
    findings.extend(unavailable);

    if solver_available {
        // ── consistency: all bounds together must be satisfiable ────────
        if !axioms.is_empty() {
            let mut vars: Vec<String> = Vec::new();
            for conj in &axioms {
                collect_vars(&conj.expr, &mut vars);
            }
            let mut script = String::from("(set-option :produce-unsat-cores true)\n");
            for var in &vars {
                script.push_str(&format!("(declare-const {var} Real)\n"));
            }
            let mut names: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (idx, conj) in axioms.iter().enumerate() {
                let name = format!("c{idx}");
                names.insert(name.clone(), idx);
                script.push_str(&format!(
                    "(assert (! {} :named {name}))\n",
                    encode_expr(&conj.expr)
                ));
            }
            script.push_str("(check-sat)\n(get-unsat-core)\n");

            match solver.solve(&script) {
                Ok(stdout) => match first_result_line(&stdout) {
                    "unsat" => {
                        unsat = true;
                        let cited: Vec<String> = parse_unsat_core(&stdout)
                            .iter()
                            .filter_map(|name| names.get(name))
                            .map(|&idx| {
                                let conj = &axioms[idx];
                                format!("`{}` ({})", conj.text, conj.loc)
                            })
                            .collect();
                        let core = parse_unsat_core(&stdout);
                        let first_idx = core.first().and_then(|name| names.get(name)).copied();
                        let mut diagnostic = Diagnostic::new(
                            codes::E046,
                            format!("contradictory bounds: {}", cited.join("; ")),
                        )
                        .with_suggestion("relax or correct the listed bounds");
                        if let Some(idx) = first_idx {
                            diagnostic = diagnostic.with_span(axioms[idx].span.clone());
                        }
                        findings.push(diagnostic);
                    }
                    "sat" => satisfiable = true,
                    _ => {
                        findings.push(Diagnostic::new(
                            codes::I098,
                            "the solver could not decide the combined bounds".to_string(),
                        ));
                    }
                },
                Err(failure) => {
                    note_failure(failure, &mut solver_runtime_failure, &mut solver_timed_out)
                }
            }
        }

        // ── entailment: bounds ⇒ claim, per claim ────────────────────────
        for claim in &claims {
            let mut vars: Vec<String> = Vec::new();
            for conj in &axioms {
                collect_vars(&conj.expr, &mut vars);
            }
            collect_vars(&claim.expr, &mut vars);

            let mut script = String::from("(set-option :produce-models true)\n");
            for var in &vars {
                script.push_str(&format!("(declare-const {var} Real)\n"));
            }
            if let Some(bounds) =
                encode_conjunction(&axioms.iter().map(|c| c.expr.clone()).collect::<Vec<_>>())
            {
                script.push_str(&format!("(assert {bounds})\n"));
            }
            script.push_str(&format!("(assert (not {}))\n", encode_expr(&claim.expr)));
            script.push_str("(check-sat)\n(get-model)\n");

            let stdout = match solver.solve(&script) {
                Ok(stdout) => stdout,
                Err(failure) => {
                    note_failure(failure, &mut solver_runtime_failure, &mut solver_timed_out);
                    continue;
                }
            };
            {
                match first_result_line(&stdout) {
                    // bounds ∧ ¬claim unsat ⇒ bounds entail the claim
                    "unsat" => {
                        claims_proved += 1;
                        proved_claim_ids.push(claim.id.clone());
                    }
                    "sat" => {
                        claims_unproved += 1;
                        let model = parse_model(&stdout);
                        let evidence = if model.is_empty() {
                            "a satisfying assignment exists".to_string()
                        } else {
                            format!(
                                "counterexample: {}",
                                render_counterexample(&model, &display_units)
                            )
                        };
                        findings.push(
                            Diagnostic::new(
                                codes::W139,
                                format!(
                                    "claim `{}` of {} is not entailed by the declared bounds ({evidence})",
                                    claim.text, claim.id
                                ),
                            )
                            .with_span(claim.span.clone())
                            .with_suggestion("strengthen the declared bounds or weaken the claim"),
                        );
                    }
                    _ => {
                        findings.push(Diagnostic::new(
                            codes::I098,
                            format!(
                                "the solver could not decide whether the declared bounds entail `{}`",
                                claim.text
                            ),
                        ));
                    }
                }
            }
        }
    }

    if let Some(after) = solver_timed_out {
        findings.push(
            Diagnostic::new(
                codes::W098,
                format!("the SMT solver timed out after {after:?}; some formal checks were skipped"),
            )
            .with_suggestion(
                "simplify the declared bounds or claims, or raise `specforge analyze --prove-timeout`",
            ),
        );
    }
    if solver_runtime_failure {
        findings.push(
            Diagnostic::new(
                codes::W098,
                "the SMT solver could not be executed; some formal checks were skipped".to_string(),
            )
            .with_suggestion("verify the z3 installation is executable"),
        );
    }

    let summary = serde_json::json!({
        "solver": solver_version,
        "solver_available": solver_available,
        "solver_runtime_failure": solver_runtime_failure,
        "entities_with_bounds": entities_with_bounds,
        "axioms": axioms.len(),
        "conjuncts": conjunct_count,
        "skipped_prose_lines": skipped_prose_lines,
        "satisfiable": satisfiable,
        "unsatisfiable": unsat,
        "claims": claims.len(),
        "claims_proved": claims_proved,
        "claims_unproved": claims_unproved,
    });
    ProveReport {
        findings,
        summary,
        proved_claim_ids,
    }
}

mod smt;
use smt::{
    collect_vars, encode_conjunction, encode_expr, first_result_line, note_display_unit,
    parse_model, parse_unsat_core, render_counterexample,
};

#[cfg(test)]
mod tests;
