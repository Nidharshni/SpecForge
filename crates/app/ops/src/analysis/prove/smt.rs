//! SMT-LIB2 over the shared expression AST: unit-normalized encoding, and reading z3's answer (its
//! result line, unsat core and model).

use specforge_parser::{Expr, SpannedExpr};

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
pub(super) fn note_display_unit(
    expr: &SpannedExpr,
    units: &mut std::collections::HashMap<String, String>,
) {
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

pub(super) fn collect_vars(expr: &SpannedExpr, vars: &mut Vec<String>) {
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

pub(super) fn encode_expr(expr: &SpannedExpr) -> String {
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

pub(super) fn encode_conjunction(parts: &[SpannedExpr]) -> Option<String> {
    let mut iter = parts.iter();
    let first = encode_expr(iter.next()?);
    let rest: Vec<String> = iter.map(encode_expr).collect();
    Some(if rest.is_empty() {
        first
    } else {
        format!("(and {first} {})", rest.join(" "))
    })
}

// ── pass ────────────────────────────────────────────────────────────────────

pub(super) fn first_result_line(stdout: &str) -> &str {
    stdout.lines().next().unwrap_or("").trim()
}

/// Extract the unsat core names from z3 stdout (a parenthesized name list
/// on the line after `unsat`).
pub(super) fn parse_unsat_core(stdout: &str) -> Vec<String> {
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
pub(super) fn parse_model(stdout: &str) -> Vec<(String, String)> {
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

pub(super) fn render_counterexample(
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
