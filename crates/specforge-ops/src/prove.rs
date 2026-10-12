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

use std::process::Command;

use specforge_common::{Diagnostic, SourceSpan, Sym, codes};
use specforge_parser::{Expr, SpannedExpr, parse_expression};
use specforge_project::passes::AnalysisContext;
use specforge_registry::ProofRole;

/// Result of one prove run over the compiled project.
pub struct ProveReport {
    pub findings: Vec<Diagnostic>,
    pub summary: serde_json::Value,
    /// Ids of entities whose formal claims were ENTAILED from the declared
    /// bounds; consumed by the coverage pass discharge funnel.
    pub proved_claim_ids: Vec<String>,
}

// ── SMT-LIB2 encoding over the shared AST ───────────────────────────────────

/// Scale factor to the unit's canonical dimension base: time →
/// milliseconds, data → bytes, everything else (including unknown units)
/// compares raw. Bounds declared with different units of the same
/// dimension therefore compare correctly (`100ms` vs `1s`).
fn unit_scale(unit: &str) -> f64 {
    match unit {
        "" | "%" => 1.0,
        // time → milliseconds
        "ms" => 1.0,
        "s" | "sec" | "secs" => 1000.0,
        "us" | "µs" => 0.001,
        "ns" => 1.0e-6,
        // data → bytes
        "b" | "B" => 1.0,
        "kb" | "KB" | "Kb" => 1.0e3,
        "mb" | "MB" | "Mb" => 1.0e6,
        "gb" | "GB" | "Gb" => 1.0e9,
        "kib" | "KiB" => 1024.0,
        "mib" | "MiB" => 1024.0 * 1024.0,
        "gib" | "GiB" => 1024.0 * 1024.0 * 1024.0,
        // unknown dimension: no normalization
        _ => 1.0,
    }
}

/// Per-variable display unit: the first unit declared alongside the
/// variable in any comparison (`latency < 100ms` pins `latency` to ms).
/// Counterexample models render in the declared unit.
fn note_display_unit(expr: &SpannedExpr, units: &mut std::collections::HashMap<String, String>) {
    match &expr.expr {
        Expr::Cmp(_, l, r) => {
            note_display_unit(l, units);
            note_display_unit(r, units);
            let var_of = |e: &SpannedExpr| matches!(&e.expr, Expr::Var(_));
            let unit_of = |e: &SpannedExpr| match &e.expr {
                Expr::Num(_, unit) => Some(unit.clone()),
                _ => None,
            };
            for (var, other) in [(l, r), (r, l)] {
                if var_of(var)
                    && let (Expr::Var(name), Some(unit)) = (&var.expr, unit_of(other))
                    && !unit.is_empty()
                    && !units.contains_key(name)
                {
                    units.insert(name.clone(), unit);
                }
            }
        }
        Expr::And(l, r) | Expr::Or(l, r) | Expr::Add(l, r) | Expr::Sub(l, r) => {
            note_display_unit(l, units);
            note_display_unit(r, units);
        }
        Expr::Neg(e) | Expr::Not(e) => note_display_unit(e, units),
        Expr::Num(_, _) | Expr::Var(_) => {}
    }
}

fn collect_vars(expr: &SpannedExpr, vars: &mut Vec<String>) {
    match &expr.expr {
        Expr::Num(_, _) => {}
        Expr::Var(name) => {
            if !vars.contains(name) {
                vars.push(name.clone());
            }
        }
        Expr::Cmp(_, l, r)
        | Expr::And(l, r)
        | Expr::Or(l, r)
        | Expr::Add(l, r)
        | Expr::Sub(l, r) => {
            collect_vars(l, vars);
            collect_vars(r, vars);
        }
        Expr::Neg(e) | Expr::Not(e) => collect_vars(e, vars),
    }
}

fn encode_expr(expr: &SpannedExpr) -> String {
    match &expr.expr {
        Expr::Num(v, unit) => {
            let scaled = v * unit_scale(unit);
            if scaled.fract() == 0.0 {
                format!("{scaled:.1}")
            } else {
                format!("{scaled}")
            }
        }
        Expr::Var(name) => name.clone(),
        Expr::Cmp(op, l, r) => format!("({} {} {})", op.as_smt(), encode_expr(l), encode_expr(r)),
        Expr::And(l, r) => format!("(and {} {})", encode_expr(l), encode_expr(r)),
        Expr::Or(l, r) => format!("(or {} {})", encode_expr(l), encode_expr(r)),
        Expr::Add(l, r) => format!("(+ {} {})", encode_expr(l), encode_expr(r)),
        Expr::Sub(l, r) => format!("(- {} {})", encode_expr(l), encode_expr(r)),
        Expr::Neg(e) => format!("(- {})", encode_expr(e)),
        Expr::Not(e) => format!("(not {})", encode_expr(e)),
    }
}

fn encode_conjunction(parts: &[SpannedExpr]) -> Option<String> {
    let mut iter = parts.iter();
    let first = encode_expr(iter.next()?);
    let rest: Vec<String> = iter.map(encode_expr).collect();
    Some(if rest.is_empty() {
        first
    } else {
        format!("(and {first} {})", rest.join(" "))
    })
}

// ── solver ──────────────────────────────────────────────────────────────────

/// Why a solver call produced no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SolveFailure {
    /// The solver ran past its timeout and was killed.
    TimedOut,
    /// The solver could not be executed or died without an answer.
    Failed,
}

/// Private seam over the SMT solver. Production: [`Z3`] (shells out with a
/// timeout). Tests: a scripted adapter. Not part of any public interface.
pub(crate) trait Solver {
    /// First line of the solver's version banner, `None` when unavailable.
    fn version(&self) -> Option<String>;
    /// Run an SMT-LIB2 script and return the solver's full stdout.
    fn solve(&self, script: &str) -> Result<String, SolveFailure>;
}

/// Options of the prove step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProveOptions {
    /// Wall-clock limit for each z3 invocation.
    pub z3_timeout: std::time::Duration,
}

impl Default for ProveOptions {
    fn default() -> Self {
        Self {
            z3_timeout: std::time::Duration::from_secs(30),
        }
    }
}

/// Production adapter: shells out to `z3`, killing it past the timeout.
struct Z3 {
    timeout: std::time::Duration,
}

impl Solver for Z3 {
    fn version(&self) -> Option<String> {
        let output = Command::new("z3").arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("z3")
                .to_string(),
        )
    }

    fn solve(&self, script: &str) -> Result<String, SolveFailure> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "specforge-prove-{}-{}.smt2",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&tmp, script).map_err(|_| SolveFailure::Failed)?;
        let result = run_with_timeout(Command::new("z3").arg(&tmp), self.timeout);
        let _ = std::fs::remove_file(&tmp);
        result
    }
}

/// Run a command to completion or kill it at `timeout`; returns its stdout.
fn run_with_timeout(
    cmd: &mut Command,
    timeout: std::time::Duration,
) -> Result<String, SolveFailure> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| SolveFailure::Failed)?;
    let mut stdout = child.stdout.take().ok_or(SolveFailure::Failed)?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(SolveFailure::TimedOut);
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(_) => return Err(SolveFailure::Failed),
        }
    }
    let buf = reader.join().map_err(|_| SolveFailure::Failed)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn first_result_line(stdout: &str) -> &str {
    stdout.lines().next().unwrap_or("").trim()
}

/// Extract the unsat core names from z3 stdout (a parenthesized name list
/// on the line after `unsat`).
fn parse_unsat_core(stdout: &str) -> Vec<String> {
    let mut lines = stdout.lines();
    if lines.next().map(str::trim) != Some("unsat") {
        return Vec::new();
    }
    lines
        .next()
        .map(|l| {
            l.trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Extract `(name, value)` pairs from a z3 model section. Handles both the
/// one-line `(define-fun x () Real 250.0)` and the pretty-printed
/// multi-line form; rational results like `(/ 1.0 4.0)` pass through raw.
fn parse_model(stdout: &str) -> Vec<(String, String)> {
    // z3 emits the result line, then the model: a bare `(`, define-fun
    // blocks (one-line or pretty-printed), and a closing `)`. The model is
    // the only content after the result line, so scan everything after it.
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for line in stdout.lines().skip(1) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("(define-fun ") {
            let toks: Vec<&str> = rest.split_whitespace().collect();
            // toks: [name, "()", "Real", value...] — value present only in
            // the one-line form; the pretty-printed form wraps to the next line
            if toks.len() >= 3 && toks[2] == "Real" {
                if toks.len() > 3 {
                    let mut value = toks[3..].join(" ");
                    if value.ends_with(')') {
                        value.pop();
                    }
                    out.push((toks[0].to_string(), value));
                    pending = None;
                } else {
                    pending = Some(toks[0].to_string());
                }
            } else {
                pending = None;
            }
            continue;
        }
        if let Some(name) = pending.take() {
            let value = t.strip_suffix(')').unwrap_or(t).trim();
            if !value.is_empty() {
                out.push((name, value.to_string()));
            }
        }
    }
    out
}

fn render_counterexample(
    model: &[(String, String)],
    display_units: &std::collections::HashMap<String, String>,
) -> String {
    model
        .iter()
        .take(4)
        .map(|(name, value)| {
            let unit = display_units.get(name).map(String::as_str).unwrap_or("");
            let scale = unit_scale(unit);
            match value.parse::<f64>() {
                Ok(v) if scale != 1.0 && !unit.is_empty() => {
                    format!("{name} = {}{unit}", v / scale)
                }
                _ => {
                    if unit.is_empty() {
                        format!("{name} = {value}")
                    } else {
                        format!("{name} = {value}{unit}")
                    }
                }
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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

fn note_failure(failure: SolveFailure, runtime_failure: &mut bool, timed_out: &mut bool) {
    match failure {
        SolveFailure::TimedOut => *timed_out = true,
        SolveFailure::Failed => *runtime_failure = true,
    }
}

/// Run the prove pass: consistency over declared bounds (E046 with unsat
/// cores) and entailment of formal claims with counterexample models (W139),
/// with `options` (the z3 timeout).
pub fn run_prove_with(ctx: &AnalysisContext, options: &ProveOptions) -> ProveReport {
    analyze_with(
        ctx,
        &Z3 {
            timeout: options.z3_timeout,
        },
    )
}

/// The prove step over an injected solver (the seam used by ops tests).
pub(crate) fn analyze_with(ctx: &AnalysisContext, solver: &dyn Solver) -> ProveReport {
    let mut findings = Vec::new();
    let mut skipped_prose_lines = 0usize;
    let mut entities_with_bounds = 0usize;
    let mut conjunct_count = 0usize;
    let mut axioms: Vec<Conjunct> = Vec::new();
    let mut claims: Vec<Claim> = Vec::new();
    let version = solver.version();
    let solver_available = version.is_some();
    let solver_version = version.unwrap_or_else(|| String::from("not found"));

    for node in ctx.graph.nodes() {
        let kind = node.kind.raw.as_str();
        let mut has_bounds = false;
        for entry in node.fields.entries() {
            let name = entry.key.as_str();
            // Only fields an extension declares a proof role for are read.
            let Some(role) = ctx
                .field_registry
                .get(kind, name)
                .and_then(|field| field.proof_role())
            else {
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
    let mut solver_timed_out = false;
    if !solver_available {
        findings.push(
            Diagnostic::new(
                codes::W098,
                "SMT solver 'z3' not found on PATH; formal entailment and consistency checks were skipped"
                    .to_string(),
            )
            .with_suggestion("install z3 (https://github.com/Z3Prover/z3) to enable `--prove`"),
        );
    }

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

    if solver_timed_out {
        findings.push(
            Diagnostic::new(
                codes::W098,
                "the SMT solver timed out; some formal checks were skipped".to_string(),
            )
            .with_suggestion("simplify the declared bounds or claims, or raise the z3 timeout"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::Sym;
    use specforge_graph::{FieldMap, Graph, Node};
    use specforge_parser::{EntityId, EntityKind, FieldValue};
    use specforge_registry::{FieldRegistry, FieldRegistryEntry, KindRegistry};

    fn span(file: &str) -> SourceSpan {
        SourceSpan {
            file: Sym::new(file),
            start_line: 1,
            start_col: 1,
            end_line: 1,
            end_col: 2,
        }
    }

    fn expr_field(text: &str) -> FieldValue {
        FieldValue::Expression(vec![parse_expression(text).expect("parse claim")])
    }

    fn node(id: &str, kind: &str, fields: FieldMap) -> Node {
        Node {
            id: EntityId { raw: Sym::new(id) },
            kind: EntityKind {
                raw: Sym::new(kind),
            },
            title: None,
            fields,
            source_span: span("spec/main.spec"),
            methods: Vec::new(),
        }
    }

    fn constraint_node(id: &str, bound: &str) -> Node {
        let mut fields = FieldMap::new();
        fields.push(Sym::new("metric"), FieldValue::String(bound.to_string()));
        node(id, "constraint", fields)
    }

    fn claim_node(id: &str, expression: &str) -> Node {
        let mut fields = FieldMap::new();
        fields.push(Sym::new("expression"), expr_field(expression));
        node(id, "invariant", fields)
    }

    /// The builtins' proof roles: a constraint's metric and an axiom's
    /// expression are bounds, an invariant's expression a claim.
    fn roles() -> FieldRegistry {
        let mut registry = FieldRegistry::default();
        for (kind, field, role) in [
            ("constraint", "metric", ProofRole::Bound),
            ("axiom", "expression", ProofRole::Bound),
            ("invariant", "expression", ProofRole::Claim),
        ] {
            registry.register(
                FieldRegistryEntry::new(
                    kind,
                    "@test/roles",
                    specforge_protocol_types::FieldDescriptor {
                        name: field.to_string(),
                        field_type: "string".to_string(),
                        normative: true,
                        proof_role: Some(role.as_str().to_string()),
                        ..Default::default()
                    },
                )
                .unwrap(),
            );
        }
        registry
    }

    fn prove(graph: &Graph) -> ProveReport {
        let kind_registry = KindRegistry::default();
        let field_registry = roles();
        let empty_proved = std::collections::HashSet::new();
        let ctx = AnalysisContext {
            graph,
            kind_registry: &kind_registry,
            field_registry: &field_registry,
            entities: &specforge_project::snapshot::EntitySnapshot::default(),
            test_results: None,
            proved_claims: Some(&empty_proved),
        };
        run_prove_with(&ctx, &ProveOptions::default())
    }

    #[test]
    fn contradictory_bounds_flagged_with_core() {
        let mut g = Graph::new();
        g.add_node(constraint_node("c1", "latency < 100ms"));
        g.add_node(constraint_node("c2", "latency > 500ms"));
        let report = prove(&g);
        assert!(report.summary["unsatisfiable"].as_bool().unwrap());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "E046" && f.message.contains("latency < 100ms"))
        );
    }

    #[test]
    fn satisfiable_bounds_prove_entailed_claim() {
        let mut g = Graph::new();
        g.add_node(constraint_node("budget", "latency < 100ms"));
        g.add_node(claim_node("inv", "latency < 200ms"));
        let report = prove(&g);
        assert!(report.summary["satisfiable"].as_bool().unwrap());
        assert_eq!(report.summary["claims"].as_u64(), Some(1));
        assert_eq!(report.summary["claims_proved"].as_u64(), Some(1));
        assert_eq!(report.summary["claims_unproved"].as_u64(), Some(0));
        assert_eq!(report.proved_claim_ids, vec!["inv".to_string()]);
        assert!(report.findings.iter().all(|f| f.code != "W139"));
    }

    #[test]
    fn unentailed_claim_yields_counterexample() {
        let mut g = Graph::new();
        g.add_node(constraint_node("budget", "latency < 100ms"));
        g.add_node(claim_node("inv", "latency > 500ms"));
        let report = prove(&g);
        assert_eq!(report.summary["claims_unproved"].as_u64(), Some(1));
        let e047 = report
            .findings
            .iter()
            .find(|f| f.code == "W139")
            .expect("W139 expected");
        assert!(
            e047.message.contains("counterexample") && e047.message.contains("latency ="),
            "counterexample must name a violating assignment: {}",
            e047.message
        );
        // the model must satisfy the bound and violate the claim
        assert!(
            e047.message.contains("latency >")
                || e047.message.contains("latency = 5")
                || e047.message.contains("latency = 1")
                || e047.message.contains("latency = 2")
                || e047.message.contains("latency = 3")
                || e047.message.contains("latency = 4")
                || e047.message.contains("latency = 6")
                || e047.message.contains("latency = 7")
                || e047.message.contains("latency = 8")
                || e047.message.contains("latency = 9"),
            "unexpected model: {}",
            e047.message
        );
    }

    #[test]
    fn claim_with_no_bounds_requires_tautology() {
        let mut g = Graph::new();
        g.add_node(claim_node("inv", "latency <= latency"));
        let report = prove(&g);
        assert_eq!(report.summary["claims_proved"].as_u64(), Some(1));

        let mut g2 = Graph::new();
        g2.add_node(claim_node("inv", "latency < 100ms"));
        let report2 = prove(&g2);
        assert_eq!(report2.summary["claims_unproved"].as_u64(), Some(1));
    }

    #[test]
    fn prose_metric_lines_are_skipped_and_counted() {
        let mut g = Graph::new();
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("metric"),
            FieldValue::String(
                "latency < 100ms\nwith up to 500 .spec files the parser stays fast".to_string(),
            ),
        );
        g.add_node(node("c1", "constraint", fields));
        let report = prove(&g);
        assert_eq!(report.summary["axioms"].as_u64(), Some(1));
        assert_eq!(report.summary["skipped_prose_lines"].as_u64(), Some(1));
        assert!(report.summary["satisfiable"].as_bool().unwrap());
    }

    #[test]
    fn cross_unit_consistency_no_false_positive() {
        // 5s = 5000ms > 200ms: consistent, must NOT be flagged
        let mut g = Graph::new();
        g.add_node(constraint_node("lo", "timeout > 200ms"));
        g.add_node(constraint_node("hi", "timeout < 5s"));
        let report = prove(&g);
        assert!(
            report.summary["satisfiable"].as_bool().unwrap(),
            "cross-unit consistent bounds must be satisfiable"
        );
        assert!(report.findings.iter().all(|f| f.code != "E046"));
    }

    #[test]
    fn cross_unit_contradiction_detected() {
        // 1s = 1000ms > 100ms: a real contradiction raw comparison misses
        let mut g = Graph::new();
        g.add_node(constraint_node("budget", "latency < 100ms"));
        g.add_node(constraint_node("floor", "latency > 1s"));
        let report = prove(&g);
        assert!(report.summary["unsatisfiable"].as_bool().unwrap());
        assert!(report.findings.iter().any(|f| f.code == "E046"));
    }

    #[test]
    fn data_units_normalize_to_bytes() {
        // 2KiB = 2048B < 1MB: consistent
        let mut g = Graph::new();
        g.add_node(constraint_node("floor", "cache_size > 2KiB"));
        g.add_node(constraint_node("budget", "cache_size < 1MB"));
        let report = prove(&g);
        assert!(report.summary["satisfiable"].as_bool().unwrap());

        // 3GB = 3e9 B > 1MB: contradiction
        let mut g2 = Graph::new();
        g2.add_node(constraint_node("budget", "cache_size < 1MB"));
        g2.add_node(constraint_node("floor", "cache_size > 3GB"));
        let report2 = prove(&g2);
        assert!(report2.summary["unsatisfiable"].as_bool().unwrap());
    }

    #[test]
    fn counterexample_renders_declared_units() {
        let mut g = Graph::new();
        g.add_node(constraint_node("budget", "latency < 100ms"));
        g.add_node(claim_node("inv", "latency > 500ms"));
        let report = prove(&g);
        let e047 = report
            .findings
            .iter()
            .find(|f| f.code == "W139")
            .expect("W139 expected");
        assert!(
            e047.message.contains("latency = ") && e047.message.contains("ms"),
            "counterexample must render in the declared unit: {}",
            e047.message
        );
        // the model value must lie in the consistent band (0..=100 ms),
        // proving the conversion happened (raw would be >= 500)
        let value: f64 = e047
            .message
            .split("latency = ")
            .nth(1)
            .and_then(|rest| rest.split("ms").next())
            .and_then(|v| v.trim().parse().ok())
            .expect("parse counterexample value");
        assert!(
            (0.0..=100.0).contains(&value),
            "model must be rendered in ms, got {value}"
        );
    }

    #[test]
    fn expr_form_field_is_consumed() {
        let mut g = Graph::new();
        let mut fields = FieldMap::new();
        fields.push(Sym::new("metric"), expr_field("latency < 100ms"));
        g.add_node(node("budget", "constraint", fields));
        g.add_node(claim_node("inv", "latency < 150ms"));
        let report = prove(&g);
        assert_eq!(report.summary["claims_proved"].as_u64(), Some(1));
    }

    #[test]
    fn a_field_without_a_proof_role_is_not_read() {
        let mut g = Graph::new();
        let mut fields = FieldMap::new();
        fields.push(
            Sym::new("metric"),
            FieldValue::String("latency < 100ms".to_string()),
        );
        // Contradicts the metric, but no extension declares it a bound.
        fields.push(
            Sym::new("threshold"),
            FieldValue::String("latency > 500ms".to_string()),
        );
        g.add_node(node("budget", "constraint", fields));
        // An `expression` on a kind no extension declares it for is no claim.
        let mut fields = FieldMap::new();
        fields.push(Sym::new("expression"), expr_field("latency > 1s"));
        g.add_node(node("login", "behavior", fields));
        let report = prove(&g);
        assert!(report.findings.iter().all(|f| f.code != "E046"));
        assert_eq!(report.summary["axioms"].as_u64(), Some(1));
        assert_eq!(report.summary["claims"].as_u64(), Some(0));
        assert_eq!(report.summary["entities_with_bounds"].as_u64(), Some(1));
    }

    #[test]
    fn an_axiom_style_bound_joins_the_bounds() {
        let mut g = Graph::new();
        let mut fields = FieldMap::new();
        fields.push(Sym::new("expression"), expr_field("latency < 100ms"));
        g.add_node(node("fast_network", "axiom", fields));
        g.add_node(claim_node("inv", "latency < 200ms"));
        let report = prove(&g);
        assert_eq!(report.summary["entities_with_bounds"].as_u64(), Some(1));
        assert_eq!(report.summary["claims"].as_u64(), Some(1));
        assert_eq!(report.proved_claim_ids, vec!["inv".to_string()]);

        // An assumption contradicting a constraint's bound is E046.
        g.add_node(constraint_node("floor", "latency > 500ms"));
        let report = prove(&g);
        let e046 = report.findings.iter().find(|f| f.code == "E046").unwrap();
        assert!(
            e046.message.starts_with("contradictory bounds: "),
            "{}",
            e046.message
        );
        assert!(
            e046.message.contains("line 1 of 'metric'"),
            "{}",
            e046.message
        );
    }

    // ── solver seam: scripted adapter, no z3 binary needed ─────────────────

    use std::cell::RefCell;

    /// Scripted solver: answers `solve` calls in order; no version models
    /// a missing z3.
    struct Scripted {
        version: Option<String>,
        answers: RefCell<Vec<Result<String, SolveFailure>>>,
    }

    impl Scripted {
        fn with(answers: Vec<Result<String, SolveFailure>>) -> Self {
            Self {
                version: Some("Z3 scripted".to_string()),
                answers: RefCell::new(answers.into_iter().rev().collect()),
            }
        }
        fn missing() -> Self {
            Self {
                version: None,
                answers: RefCell::new(Vec::new()),
            }
        }
    }

    impl Solver for Scripted {
        fn version(&self) -> Option<String> {
            self.version.clone()
        }
        fn solve(&self, _script: &str) -> Result<String, SolveFailure> {
            self.answers
                .borrow_mut()
                .pop()
                .expect("scripted solver ran out of answers")
        }
    }

    fn prove_with(graph: &Graph, solver: &dyn Solver) -> ProveReport {
        let kind_registry = KindRegistry::default();
        let field_registry = roles();
        let empty_proved = std::collections::HashSet::new();
        let ctx = AnalysisContext {
            graph,
            kind_registry: &kind_registry,
            field_registry: &field_registry,
            entities: &specforge_project::snapshot::EntitySnapshot::default(),
            test_results: None,
            proved_claims: Some(&empty_proved),
        };
        analyze_with(&ctx, solver)
    }

    #[test]
    fn timed_out_solver_yields_w098() {
        let mut g = Graph::new();
        g.add_node(constraint_node("c1", "latency < 100ms"));
        let report = prove_with(&g, &Scripted::with(vec![Err(SolveFailure::TimedOut)]));
        let w098 = report
            .findings
            .iter()
            .find(|f| f.code == "W098")
            .expect("W098 expected");
        assert!(w098.message.contains("timed out"), "{}", w098.message);
    }

    #[test]
    fn missing_solver_yields_w098_without_solving() {
        let mut g = Graph::new();
        g.add_node(constraint_node("c1", "latency < 100ms"));
        let report = prove_with(&g, &Scripted::missing());
        assert!(report.findings.iter().any(|f| f.code == "W098"));
        assert_eq!(report.summary["solver_available"], false);
    }

    #[test]
    fn solver_failing_mid_run_yields_w098() {
        let mut g = Graph::new();
        g.add_node(constraint_node("c1", "latency < 100ms"));
        g.add_node(claim_node("inv", "latency < 200ms"));
        let report = prove_with(
            &g,
            &Scripted::with(vec![Ok("sat\n".to_string()), Err(SolveFailure::Failed)]),
        );
        assert!(report.findings.iter().any(|f| f.code == "W098"));
        assert_eq!(report.summary["solver_runtime_failure"], true);
        assert!(report.proved_claim_ids.is_empty());
    }

    #[test]
    fn unsat_bounds_yield_e046_with_scripted_core() {
        let mut g = Graph::new();
        g.add_node(constraint_node("c1", "latency < 100ms"));
        g.add_node(constraint_node("c2", "latency > 500ms"));
        let report = prove_with(
            &g,
            &Scripted::with(vec![Ok("unsat\n(c0 c1)\n".to_string())]),
        );
        assert!(report.findings.iter().any(|f| f.code == "E046"
            && f.message.contains("latency < 100ms")
            && f.message.contains("latency > 500ms")));
        assert_eq!(report.summary["unsatisfiable"], true);
    }

    #[test]
    fn process_outliving_the_timeout_is_killed() {
        let started = std::time::Instant::now();
        let out = run_with_timeout(
            std::process::Command::new("sleep").arg("30"),
            std::time::Duration::from_millis(200),
        );
        assert!(matches!(out, Err(SolveFailure::TimedOut)));
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }
}
