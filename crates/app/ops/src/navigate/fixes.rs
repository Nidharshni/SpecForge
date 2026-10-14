//! The edits that fix a diagnostic, read from its data or the graph, never
//! its message: what the LSP offers as code actions and MCP's
//! `specforge.suggest_fixes` returns, the same fixes on both surfaces.

use std::collections::BTreeSet;

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticData, SourceSpan, Sym};
use specforge_graph::Node;

use super::text::SourceText;
use super::{Navigator, is_about, overlaps};

/// One edit: `span`'s text becomes `new_text` (an empty span inserts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub span: SourceSpan,
    pub new_text: String,
}

/// What a fix is to an editor: a quick fix of the diagnostic, or a
/// refactoring that adds to the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "lowercase")]
pub enum FixKind {
    /// `quickfix`, the LSP `CodeActionKind`.
    QuickFix,
    /// `refactor`.
    Refactor,
}

/// Where a fix comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixSource {
    /// An unresolved reference (E003) or import (E025) whose data names a
    /// close match: the token becomes the match.
    ReplaceUnresolved,
    /// An unresolved reference to an id no entity has, in a field that
    /// targets a kind: a stub of that kind at the end of the file.
    CreateStub,
    /// An entity that declares no verify statements and wants some: it
    /// owes obligations (a `no_verify_statements` rule applies to its kind)
    /// or its kind is testable, and neither a union body nor an exempting
    /// flag exempts it (ADR 0019). A verify stub in its block.
    AddVerifyStub,
}

/// A fix: a title, and the edits that apply it (at least one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub kind: FixKind,
    pub source: FixSource,
    /// The code of the diagnostic it fixes: the diagnostic's own, or, for
    /// a verify stub, the code of the rule that reports its entity (the
    /// `no_verify_statements` rule that obliges its kind), and none when
    /// no rule reports it.
    pub diagnostic_code: Option<String>,
    /// The entity it is about.
    pub subject: Option<Sym>,
    pub edits: Vec<TextEdit>,
    /// Where the fix applies: its diagnostic's span, else its subject's
    /// block (what `FixQuery::within` overlaps).
    pub anchor: Option<SourceSpan>,
}

/// Which fixes a question asks for: every filter given must hold.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixQuery<'q> {
    /// Fixes about this entity: its diagnostics' ([`is_about`]) and its
    /// verify stub.
    pub entity: Option<&'q str>,
    /// Fixes whose diagnostic (or, for a verify stub, whose entity) is in
    /// this spec file.
    pub file: Option<&'q str>,
    /// Fixes of diagnostics with this code.
    pub code: Option<&'q str>,
    /// Fixes whose diagnostic span, or whose subject's block, overlaps
    /// this range (an editor's cursor or selection).
    pub within: Option<&'q SourceSpan>,
}

impl<F: Fn(&str) -> Option<String>> Navigator<'_, F> {
    /// The fixes `diagnostics` (and the graph's entities missing verify
    /// statements) offer, filtered by `query`, sorted by where they apply
    /// then title. Every fix has at least one edit; a diagnostic whose
    /// data names no fix offers none.
    pub fn fixes(&self, diagnostics: &[Diagnostic], query: &FixQuery) -> Vec<Fix> {
        let graph = self.view.graph();
        let mut fixes = Vec::new();
        let mut stubbed: BTreeSet<String> = BTreeSet::new();
        for diagnostic in diagnostics {
            if query.code.is_some_and(|code| code != diagnostic.code)
                || query
                    .entity
                    .is_some_and(|id| !is_about(graph, diagnostic, id))
                || query
                    .file
                    .is_some_and(|file| diagnostic.span.as_ref().is_none_or(|s| s.file != file))
                || query.within.is_some_and(|range| {
                    diagnostic.span.as_ref().is_none_or(|s| !overlaps(s, range))
                })
            {
                continue;
            }
            fixes.extend(self.replacement(diagnostic));
            fixes.extend(self.entity_stub(diagnostic, &mut stubbed));
        }
        let verify_stubs = graph
            .nodes()
            .into_iter()
            .filter(|n| query.entity.is_none_or(|id| n.id.raw == id))
            .filter(|n| query.file.is_none_or(|file| n.source_span.file == file))
            .filter(|n| {
                query
                    .within
                    .is_none_or(|range| overlaps(&n.source_span, range))
            })
            .filter_map(|n| self.verify_stub(n))
            .filter(|fix| {
                query
                    .code
                    .is_none_or(|code| fix.diagnostic_code.as_deref() == Some(code))
            });
        fixes.extend(verify_stubs);
        fixes.sort_by(|a, b| {
            let at = |fix: &Fix| {
                fix.anchor
                    .as_ref()
                    .map(|s| (s.file, s.start_line, s.start_col))
            };
            at(a).cmp(&at(b)).then_with(|| a.title.cmp(&b.title))
        });
        fixes
    }

    /// An unresolved reference or import whose data names a close match:
    /// its token, once the text spells it there, becomes the match.
    fn replacement(&self, diagnostic: &Diagnostic) -> Option<Fix> {
        let span = diagnostic.span.as_ref()?;
        let text = self.text(span.file)?;
        let (edit_span, candidate, subject) = match diagnostic.data.as_deref()? {
            DiagnosticData::UnresolvedReference {
                target,
                entity,
                did_you_mean: Some(candidate),
                ..
            } => {
                text.spells(span, target).then_some(())?;
                (span.clone(), candidate, Some(Sym::new(entity)))
            }
            DiagnosticData::UnresolvedImport {
                path,
                did_you_mean: Some(candidate),
            } => (quoted(&text, span, path)?, candidate, None),
            _ => return None,
        };
        Some(Fix {
            title: format!("Replace with '{candidate}'"),
            kind: FixKind::QuickFix,
            source: FixSource::ReplaceUnresolved,
            diagnostic_code: Some(diagnostic.code.clone()),
            subject,
            edits: vec![TextEdit {
                span: edit_span,
                new_text: candidate.clone(),
            }],
            anchor: Some(span.clone()),
        })
    }

    /// An unresolved reference to an id no entity has, in a field whose
    /// registry entry targets a kind: a stub of that kind, appended to the
    /// file holding the reference, once per target.
    fn entity_stub(&self, diagnostic: &Diagnostic, stubbed: &mut BTreeSet<String>) -> Option<Fix> {
        let DiagnosticData::UnresolvedReference {
            target,
            entity,
            field,
            ..
        } = diagnostic.data.as_deref()?
        else {
            return None;
        };
        let graph = self.view.graph();
        if graph.node(target).is_some() || stubbed.contains(target) {
            return None;
        }
        let holder = graph.node(entity)?;
        let kind = self
            .view
            .registries()
            .fields()
            .get(holder.kind.raw.as_str(), field)?
            .declared()
            .target_kind
            .clone()?;
        let file = diagnostic
            .span
            .as_ref()
            .map_or(holder.source_span.file, |s| s.file);
        let end = self.text(file)?.end(file);
        stubbed.insert(target.clone());
        Some(Fix {
            title: format!("Create {kind} stub for {target}"),
            kind: FixKind::Refactor,
            source: FixSource::CreateStub,
            diagnostic_code: Some(diagnostic.code.clone()),
            subject: Some(holder.id.raw),
            edits: vec![TextEdit {
                span: end,
                new_text: format!(
                    "\n{kind} {target} \"{target}\" {{\n  // TODO: fill in fields\n}}\n"
                ),
            }],
            anchor: diagnostic.span.clone(),
        })
    }

    /// An entity of a kind that supports verify statements, with none, that
    /// wants some (ADR 0019): it owes obligations, or its kind is testable,
    /// and nothing exempts it. A union body or an exempting flag is offered
    /// none: a stub there is no obligation it owes (a union has no block to
    /// hold one). The stub is of the kind's first allowed verify kind
    /// (`unit` when it names none), inserted before the block's closing
    /// brace; it fixes the rule that obliges the entity's kind, if any.
    fn verify_stub(&self, node: &Node) -> Option<Fix> {
        let registries = self.view.registries();
        let kind = registries
            .kinds()
            .get(node.kind.raw.as_str())
            .filter(|entry| entry.supports_verify)?;
        let standing = self.view.entities().standing(node.id.raw.as_str())?;
        if !standing.wants_obligations() {
            return None;
        }
        let verify_kind = kind
            .allowed_verify_kinds
            .first()
            .map_or("unit", String::as_str);
        let stub = format!("  verify {verify_kind} \"{} — TODO\"\n", node.id.raw);
        let block = &node.source_span;
        let edit = match self
            .text(block.file)
            .and_then(|text| closing_brace(&text, block))
        {
            // The brace opens its own line: the stub goes on the line
            // before it.
            Some((brace, true)) => TextEdit {
                span: SourceSpan {
                    start_col: 1,
                    end_col: 1,
                    ..brace
                },
                new_text: stub,
            },
            // The brace follows code on its line (a one-line block): the
            // stub goes on a line of its own before it.
            Some((brace, false)) => TextEdit {
                span: SourceSpan {
                    end_line: brace.start_line,
                    end_col: brace.start_col,
                    ..brace
                },
                new_text: format!("\n{stub}"),
            },
            None => TextEdit {
                span: SourceSpan {
                    file: block.file,
                    start_line: block.end_line,
                    start_col: 1,
                    end_line: block.end_line,
                    end_col: 1,
                },
                new_text: stub,
            },
        };
        // The rule that reports it, if any.
        let code = standing.reported_by().map(str::to_string);
        Some(Fix {
            title: format!("Add verify stub for {}", node.id.raw),
            kind: FixKind::QuickFix,
            source: FixSource::AddVerifyStub,
            diagnostic_code: code,
            subject: Some(node.id.raw),
            edits: vec![edit],
            anchor: Some(block.clone()),
        })
    }
}

/// The span of `path` inside its quotes, on the lines of `span` (an import
/// statement).
fn quoted(text: &SourceText, span: &SourceSpan, path: &str) -> Option<SourceSpan> {
    let lines = text.lines(span.start_line, span.end_line)?;
    let haystack = text.slice(lines.start, lines.end)?;
    let at = haystack.find(&format!("\"{path}\""))? + lines.start + 1;
    Some(text.span(span.file, at, at + path.len()))
}

/// The block's closing brace (its last `}` outside strings and comments)
/// as a one-byte span, and whether it is the first thing on its line.
fn closing_brace(text: &SourceText, block: &SourceSpan) -> Option<(SourceSpan, bool)> {
    let brace = text
        .tokens(block)
        .into_iter()
        .rev()
        .find(|t| t.is_punct('}'))?;
    let span = text.span(block.file, brace.start, brace.end);
    let line_start = text.offset(span.start_line, 1)?;
    let alone = text
        .slice(line_start, brace.start)?
        .chars()
        .all(char::is_whitespace);
    Some((span, alone))
}
