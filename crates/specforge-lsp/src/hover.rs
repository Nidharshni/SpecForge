//! The hover's markdown: the diagnostics under the cursor, an entity's
//! facts (the inspect read view, `specforge_ops::inspect`) and a field's
//! help. Rendering only: the facts come from the read view, so the hover
//! and MCP `specforge.inspect` cannot disagree (ADR 0015, "Inspect").

use crate::document::LineIndex;
use specforge_common::Diagnostic;
use specforge_ops::OpError;
use specforge_ops::inspect::{EntityCoverage, EntityFacts};
use specforge_parser::FieldValue;
use specforge_registry::FieldRegistry;
use std::collections::BTreeMap;
use tower_lsp::lsp_types::Position;

/// The rule between the sections of a hover.
pub const SECTION: &str = "\n\n---\n\n";

/// The published diagnostics whose range (in the document `index`
/// indexes) holds `position`, in published order.
pub fn diagnostics_at<'d>(
    published: &'d [Diagnostic],
    index: &LineIndex,
    position: Position,
) -> Vec<&'d Diagnostic> {
    let at = (position.line, position.character);
    published
        .iter()
        .filter(|diag| {
            diag.span.as_ref().is_some_and(|span| {
                let range = index.range(span);
                (range.start.line, range.start.character) <= at
                    && at <= (range.end.line, range.end.character)
            })
        })
        .collect()
}

/// Markdown for the diagnostics `shown` under the cursor
/// ([`diagnostics_at`]): each code with the catalogue's title, the message,
/// the catalogue's explanation and the docs link; a code the catalogue
/// doesn't have shows its code and message only. `None` when none is.
pub fn diagnostics(shown: &[&Diagnostic]) -> Option<String> {
    let sections: Vec<String> = shown
        .iter()
        .map(
            |diag| match specforge_diagnostics::describes(&diag.code, diag.origin()) {
                Some(entry) => {
                    let mut section = format!(
                        "**{}** · {}\n\n{}\n\n{}",
                        entry.code, entry.title, diag.message, entry.explanation
                    );
                    if let Some(href) = specforge_diagnostics::docs_href(entry.code) {
                        section.push_str(&format!("\n\n[Documentation]({href})"));
                    }
                    section
                }
                // An extension's own uncatalogued code, or one it may not use
                // (W150): no catalogue entry describes it.
                None => match diag.origin() {
                    Some(extension) => format!(
                        "**{}** · reported by '{extension}'\n\n{}",
                        diag.code, diag.message
                    ),
                    None => format!("**{}**\n\n{}", diag.code, diag.message),
                },
            },
        )
        .collect();
    (!sections.is_empty()).then(|| sections.join(SECTION))
}

/// Markdown for an entity's facts:
/// - its kind, ID and title; its kind's description, declaring extension
///   and badges (`testable` is the standing inspect reports); the
///   statement its extension declares headline, quoted whole;
/// - **Coverage**: its coverage, or why it does not count, or that the
///   recorded report cannot be read; while the project rebuilds
///   (`rebuilding`), that coverage is unavailable. None for an entity of a
///   kind that is not testable and that declares no obligations;
/// - **Refers to**: its references, grouped by field;
/// - **Referenced by**: the references to it, grouped by the referencing
///   kind and field;
/// - **Fields**: its field values but the headline;
/// - **Diagnostics**: the diagnostics about it that are not among those
///   `shown` for the cursor, each code linked to its catalogue entry.
pub fn entity(
    facts: &EntityFacts,
    coverage: &Result<EntityCoverage, OpError>,
    shown: &[&Diagnostic],
    rebuilding: bool,
) -> String {
    let node = facts.node;
    let title = node
        .title
        .as_deref()
        .map(|t| format!(" — {t}"))
        .unwrap_or_default();

    // Section 1: Header + description + extension badges. The header
    // carries no editor markup: the icon a kind declares (a SymbolKind name)
    // reaches editors through document and workspace symbols, and a client
    // adds its own icon (ADR 0015, I4).
    let mut header_section = format!("**{}** `{}`{}", node.kind.raw, node.id.raw, title);
    if let Some(entry) = facts.kind {
        if let Some(ref desc) = entry.declared.description {
            header_section.push_str(&format!("\n\n{}", desc));
        }
        let mut ext_line = format!("*{}*", entry.source_extension);
        if facts.standing.testable {
            ext_line.push_str(" · `testable`");
        }
        if entry.supports_verify {
            ext_line.push_str(" · `verify`");
        }
        if entry.declared.singleton {
            ext_line.push_str(" · `singleton`");
        }
        header_section.push_str(&format!("\n{}", ext_line));
    }
    if let Some(headline) = facts.headline {
        header_section.push_str(&format!("\n\n{}", quoted(headline)));
    }

    let mut sections: Vec<String> = vec![header_section];

    // Section 2: its coverage
    if let Some(line) = coverage_line(facts, coverage, rebuilding) {
        sections.push(format!("**Coverage** · {line}"));
    }

    // Section 3: its references (Refers to), by field
    let outgoing = &facts.references.outgoing;
    if !outgoing.is_empty() {
        let mut by_field: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for reference in outgoing {
            by_field
                .entry(reference.field.as_str())
                .or_default()
                .push(reference.peer.as_str());
        }
        let mut section = format!("**Refers to** *({})*", outgoing.len());
        for (field, targets) in &by_field {
            section.push_str(&format!("\n- `{}` → {}", field, targets.join(", ")));
        }
        sections.push(section);
    }

    // Section 4: the references to it (Referenced by), by kind and field
    let incoming = &facts.references.incoming;
    if !incoming.is_empty() {
        let mut by_kind_field: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
        for reference in incoming {
            let kind = reference.peer_kind.map_or("unknown", |k| k.as_str());
            by_kind_field
                .entry((kind, reference.field.as_str()))
                .or_default()
                .push(reference.peer.as_str());
        }
        let mut section = format!("**Referenced by** *({})*", incoming.len());
        for ((kind, field), sources) in &by_kind_field {
            section.push_str(&format!(
                "\n- {} via `{}`: {}",
                kind,
                field,
                sources.join(", ")
            ));
        }
        sections.push(section);
    }

    // Section 5: Fields, but the headline, which the summary quotes
    let fields: Vec<String> = node
        .fields
        .entries()
        .iter()
        .filter(|entry| entry.key.as_str() != "title")
        .filter(|entry| !is_headline(&entry.value, facts.headline))
        .map(|entry| format!("- `{}` = {}", entry.key, format_field_value(&entry.value)))
        .collect();
    if !fields.is_empty() {
        sections.push(format!("**Fields**\n{}", fields.join("\n")));
    }

    // Section 6: the diagnostics about it the cursor's do not already show
    let listed: Vec<String> = facts
        .diagnostics
        .iter()
        .filter(|diagnostic| !shown.contains(diagnostic))
        .map(|diagnostic| {
            let described = specforge_diagnostics::describes(&diagnostic.code, diagnostic.origin());
            let code =
                match described.and_then(|entry| specforge_diagnostics::docs_href(entry.code)) {
                    Some(href) => format!("[**{}**]({href})", diagnostic.code),
                    None => format!("**{}**", diagnostic.code),
                };
            format!("- {code} {}", diagnostic.message)
        })
        .collect();
    if !listed.is_empty() {
        sections.push(format!(
            "**Diagnostics** *({})*\n{}",
            listed.len(),
            listed.join("\n")
        ));
    }

    sections.join(SECTION)
}

/// Whether `value` is the headline statement: the very string the read
/// view borrowed from the node, not merely an equal one.
fn is_headline(value: &FieldValue, headline: Option<&str>) -> bool {
    match (value, headline) {
        (FieldValue::String(s), Some(headline)) => std::ptr::eq(s.as_str(), headline),
        _ => false,
    }
}

/// `text` as a markdown quote, whole: its blank edge lines trimmed, its
/// common leading whitespace removed, every line prefixed `> `.
fn quoted(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return ">".to_string();
    };
    let lines = &lines[first..=last];
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            let line = l.get(indent..).unwrap_or_else(|| l.trim_start()).trim_end();
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The Coverage line after `**Coverage** · `: as `specforge.inspect`
/// reports the entity's coverage. `None` for an entity whose kind is not
/// testable and that declares no obligations.
fn coverage_line(
    facts: &EntityFacts,
    coverage: &Result<EntityCoverage, OpError>,
    rebuilding: bool,
) -> Option<String> {
    let standing = facts.standing;
    let declares = standing.declares_obligations();
    if !standing.testable && !declares {
        return None;
    }
    // The stand-in view has no root: it cannot read the recorded report,
    // and saying there is none would be false (panel D12).
    if rebuilding {
        return Some("unavailable while the project rebuilds".to_string());
    }
    if standing.exempt() {
        return Some(if standing.obligated() {
            "exempt: it owes none (a union or an exempting field)".to_string()
        } else {
            "exempt: its kind need not declare obligations".to_string()
        });
    }
    let coverage = match coverage {
        Ok(coverage) => coverage,
        Err(error) => {
            return Some(format!(
                "the recorded test report cannot be read ({}): {error}",
                error.code
            ));
        }
    };
    let verdict = &coverage.verdict;
    let mut line = format!(
        "`{}` · {}/{} obligations proven",
        specforge_ops::coverage::STATUS.name_of(coverage.status()),
        verdict.proven,
        verdict.obligations
    );
    if coverage.recorded {
        let plural = if verdict.tests == 1 { "" } else { "s" };
        line.push_str(&format!(" · {} test{plural}", verdict.tests));
        if verdict.failing > 0 {
            line.push_str(&format!(" · {} failing", verdict.failing));
        }
    } else {
        line.push_str(" · no test report recorded");
    }
    if !standing.testable {
        line.push_str(" · its kind is not testable, so it does not count");
    }
    Some(line)
}

/// Returns markdown-formatted hover content for a field name within an entity block.
pub fn hover_field_info(
    field_name: &str,
    entity_kind: &str,
    field_registry: &FieldRegistry,
) -> Option<String> {
    let entry = field_registry.get(entity_kind, field_name)?;

    let type_str = entry.type_label();

    // First line: field name + type, with optional target kind on same line
    let first_line = if let Some(ref target) = entry.declared().target_kind {
        format!("**`{}`** : {} → **{}**", field_name, type_str, target)
    } else {
        format!("**`{}`** : {}", field_name, type_str)
    };

    let mut parts = vec![first_line];

    if let Some(ref desc) = entry.declared().description {
        parts.push(desc.clone());
    }
    if let Some(ref default) = entry.declared().default_value {
        parts.push(format!("Absent, it counts as `{default}`."));
    }

    // Edge and required on same line
    match (&entry.declared().edge, entry.declared().required) {
        (Some(edge_name), true) => {
            parts.push(format!("Edge `{}` · *required*", edge_name));
        }
        (Some(edge_name), false) => {
            parts.push(format!("Edge `{}`", edge_name));
        }
        (None, true) => {
            parts.push("*required*".to_string());
        }
        (None, false) => {}
    }

    parts.push(format!("*{}*", entry.source_extension()));

    Some(parts.join("  \n"))
}

fn format_field_value(fv: &FieldValue) -> String {
    match fv {
        FieldValue::String(s) => {
            // At most 120 bytes, cut at the last character boundary at or
            // before byte 120: a cut inside a character would panic.
            let truncated = if s.len() > 120 {
                format!("{}…", &s[..s.floor_char_boundary(120)])
            } else {
                s.clone()
            };
            format!("\"{}\"", truncated)
        }
        FieldValue::Identifier(s) => format!("`{}`", s),
        FieldValue::TypeUnion(types) => types.join(" | "),
        FieldValue::Expression(exprs) => {
            if exprs.len() == 1 {
                format!("expr {{ {} }}", exprs[0])
            } else {
                format!("expr {{ {} bounds }}", exprs.len())
            }
        }
        FieldValue::Integer(n) => n.to_string(),
        FieldValue::Boolean(b) => b.to_string(),
        FieldValue::Date(d) => d.clone(),
        FieldValue::ReferenceList(refs) => {
            format!(
                "[{}]",
                refs.iter()
                    .map(|r| r.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        FieldValue::StringList(items) => {
            if items.len() <= 5 {
                format!(
                    "[{}]",
                    items
                        .iter()
                        .map(|s| format!("\"{}\"", s))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                let shown: Vec<_> = items[..5].iter().map(|s| format!("\"{}\"", s)).collect();
                format!("[{}, … +{}]", shown.join(", "), items.len() - 5)
            }
        }
        FieldValue::VariantList(variants) => format!("[{}]", variants.join(" | ")),
        FieldValue::MixedList(items) => {
            let formatted: Vec<_> = items.iter().map(format_field_value).collect();
            format!("[{}]", formatted.join(", "))
        }
        FieldValue::Block(map) => {
            let count = map.entries().len();
            format!("{{…}} ({} fields)", count)
        }
        FieldValue::VerifyList(stmts) => {
            let items: Vec<_> = stmts
                .iter()
                .map(|v| format!("{}: {}", v.kind, v.description))
                .collect();
            if items.len() <= 3 {
                items.join("; ")
            } else {
                format!("{}; … +{}", items[..3].join("; "), items.len() - 3)
            }
        }
    }
}
