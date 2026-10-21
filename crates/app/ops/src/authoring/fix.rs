//! The edits that fix a diagnostic, read from its data or the graph, never
//! its message: what the LSP offers as code actions and MCP's
//! `specforge.suggest_fixes` returns, the same fixes on both surfaces
//! (ADR 0016). They live in `authoring`, beside rename, because a stub is a
//! typed edit (ADR 0151) planned over the text the project was compiled
//! from, offered as the position edits that apply it, and not offered when
//! its plan is refused (ADR 0161); `read` may not use `authoring` (ADR 0086).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticData, SourceSpan, Sym};
use specforge_graph::Node;

use crate::authoring::edit::{self, Edit, EditField, Input};
use crate::read::navigate::{Navigator, SourceText, TextEdit, is_about, overlaps};
use crate::view::{EntityField, ProjectView};
use crate::{OpError, OpErrorKind, Refusal};

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
    /// targets a kind: the typed create of a block of that kind in the
    /// reference's file (after its last block of the kind), each required
    /// field `TODO`; not offered when the create is refused.
    CreateStub,
    /// An entity that declares no verify statements and wants some: it
    /// owes obligations (a `no_verify_statements` rule applies to its kind)
    /// or its kind is testable, and neither a union body nor an exempting
    /// flag exempts it (ADR 0019). A verify stub in its block: the typed add_verify edit.
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
    /// verify stub. [`fixes`] refuses one the graph lacks
    /// (`view::not_found`); [`offers`] takes it as a plain filter.
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

/// A fix chosen for a diagnostic or an entity, its edits not yet made: a
/// replacement carries its token edit, a stub its typed edit. Choosing costs
/// what choosing a replacement costs; making a stub's edits costs one compile
/// of the edited project (ADR 0161).
#[derive(Debug, Clone)]
pub struct Offer {
    /// The fix; a stub's edits are empty until [`Offer::fix`].
    fix: Fix,
    typed: Option<Edit>,
}

impl Offer {
    pub fn title(&self) -> &str {
        &self.fix.title
    }

    pub fn kind(&self) -> FixKind {
        self.fix.kind
    }

    pub fn anchor(&self) -> Option<&SourceSpan> {
        self.fix.anchor.as_ref()
    }

    /// Whether its edits are made by [`Offer::fix`] (a stub) rather than carried (a replacement).
    pub fn is_typed(&self) -> bool {
        self.typed.is_some()
    }

    /// The fix with its edits; a stub whose plan is refused is the refusal (E080, invalid_name, ...).
    pub fn fix(self, navigator: &Navigator<'_>) -> Result<Fix, Refusal<EditField>> {
        match self.typed {
            None => Ok(self.fix),
            Some(edit) => Ok(Fix {
                edits: typed(navigator, edit)?,
                ..self.fix
            }),
        }
    }
}

/// The fixes `diagnostics` (and the graph's entities missing verify
/// statements) offer, filtered by `query`, sorted by where they apply
/// then title. Every fix has at least one edit; a diagnostic whose
/// data names no fix offers none. A stub is planned (one compile of the
/// edited project, E080): one whose plan is refused is not offered. A
/// `query.entity` the graph lacks is [`crate::view::not_found`] (E003),
/// refused before any stub is planned.
pub fn fixes(
    navigator: &Navigator<'_>,
    diagnostics: &[Diagnostic],
    query: &FixQuery,
) -> Result<Vec<Fix>, Refusal<EntityField>> {
    if let Some(id) = query.entity {
        navigator.view().entity(id, EntityField::EntityId)?;
    }
    Ok(offers(navigator, diagnostics, query)
        .into_iter()
        .filter_map(|offer| offer.fix(navigator).ok())
        .collect())
}

/// The fixes [`fixes`] offers, chosen but not made: what a client that
/// resolves them later is shown.
pub fn offers(
    navigator: &Navigator<'_>,
    diagnostics: &[Diagnostic],
    query: &FixQuery,
) -> Vec<Offer> {
    let graph = navigator.view().graph();
    let mut fixes: Vec<Offer> = Vec::new();
    let mut stubbed: BTreeSet<String> = BTreeSet::new();
    for diagnostic in diagnostics {
        if query.code.is_some_and(|code| code != diagnostic.code)
            || query
                .entity
                .is_some_and(|id| !is_about(graph, diagnostic, id))
            || query
                .file
                .is_some_and(|file| diagnostic.span.as_ref().is_none_or(|s| s.file != file))
            || query
                .within
                .is_some_and(|range| diagnostic.span.as_ref().is_none_or(|s| !overlaps(s, range)))
        {
            continue;
        }
        fixes.extend(replacement(navigator, diagnostic).map(|fix| Offer { fix, typed: None }));
        fixes.extend(entity_stub(navigator, diagnostic, &mut stubbed));
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
        .filter_map(|n| verify_stub(navigator, n))
        .filter(|offer| {
            query
                .code
                .is_none_or(|code| offer.fix.diagnostic_code.as_deref() == Some(code))
        });
    fixes.extend(verify_stubs);
    fixes.sort_by(|a, b| {
        let at = |offer: &Offer| offer.anchor().map(|s| (s.file, s.start_line, s.start_col));
        at(a).cmp(&at(b)).then_with(|| a.title().cmp(b.title()))
    });
    fixes
}

/// An unresolved reference or import whose data names a close match:
/// its token, once the text spells it there, becomes the match.
fn replacement(navigator: &Navigator<'_>, diagnostic: &Diagnostic) -> Option<Fix> {
    let span = diagnostic.span.as_ref()?;
    let text = navigator.text(span.file)?;
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
/// registry entry targets a kind: the typed create of a block of that kind
/// in the file holding the reference, once per target; none when the
/// create is refused.
fn entity_stub(
    navigator: &Navigator<'_>,
    diagnostic: &Diagnostic,
    stubbed: &mut BTreeSet<String>,
) -> Option<Offer> {
    let DiagnosticData::UnresolvedReference {
        target,
        entity,
        field,
        ..
    } = diagnostic.data.as_deref()?
    else {
        return None;
    };
    let graph = navigator.view().graph();
    if graph.node(target).is_some() || stubbed.contains(target) {
        return None;
    }
    let holder = graph.node(entity)?;
    let kind = navigator
        .view()
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
    let create = Edit::Create {
        kind: kind.clone(),
        id: target.clone(),
        title: target.clone(),
        file: file.to_string(),
        fields: placeholders(navigator.view(), &kind),
        verify: Vec::new(),
    };
    stubbed.insert(target.clone());
    Some(Offer {
        fix: Fix {
            title: format!("Create {kind} stub for {target}"),
            kind: FixKind::Refactor,
            source: FixSource::CreateStub,
            diagnostic_code: Some(diagnostic.code.clone()),
            subject: Some(holder.id.raw),
            edits: Vec::new(),
            anchor: diagnostic.span.clone(),
        },
        typed: Some(create),
    })
}

/// An entity of a kind that supports verify statements, with none, that
/// wants some (ADR 0019): it owes obligations, or its kind is testable,
/// and nothing exempts it. A union body or an exempting flag is offered
/// none: a stub there is no obligation it owes (a union has no block to
/// hold one). The stub is of the kind's first allowed verify kind
/// (`unit` when it names none), placed by the typed add_verify edit;
/// it fixes the rule that obliges the entity's kind, if any.
fn verify_stub(navigator: &Navigator<'_>, node: &Node) -> Option<Offer> {
    let registries = navigator.view().registries();
    let kind = registries
        .kinds()
        .get(node.kind.raw.as_str())
        .filter(|entry| entry.supports_verify)?;
    let standing = navigator.view().entities().standing(node.id.raw.as_str())?;
    if !standing.wants_obligations() {
        return None;
    }
    let verify_kind = kind
        .allowed_verify_kinds
        .first()
        .map_or("unit", String::as_str);
    let add = Edit::AddVerify {
        entity_id: node.id.raw.to_string(),
        kind: Some(verify_kind.to_string()),
        text: format!("{} — TODO", node.id.raw),
    };
    let block = &node.source_span;
    // The rule that reports it, if any.
    let code = standing.reported_by().map(str::to_string);
    Some(Offer {
        fix: Fix {
            title: format!("Add verify stub for {}", node.id.raw),
            kind: FixKind::QuickFix,
            source: FixSource::AddVerifyStub,
            diagnostic_code: code,
            subject: Some(node.id.raw),
            edits: Vec::new(),
            anchor: Some(block.clone()),
        },
        typed: Some(add),
    })
}

/// The span of `path` inside its quotes, on the lines of `span` (an import
/// statement).
fn quoted(text: &SourceText, span: &SourceSpan, path: &str) -> Option<SourceSpan> {
    let lines = text.lines(span.start_line, span.end_line)?;
    let haystack = text.slice(lines.start, lines.end)?;
    let at = haystack.find(&format!("\"{path}\""))? + lines.start + 1;
    Some(text.span(span.file, at, at + path.len()))
}

/// What a create stub writes in each field its kind requires.
const PLACEHOLDER: &str = "TODO";

/// Each field a new `kind` block must write (the registry's `required`), as `TODO`. A required field that cannot
/// hold it (an enum, a reference, a block) makes the create's plan refuse, and the stub is not offered.
fn placeholders(view: &ProjectView<'_>, kind: &str) -> BTreeMap<String, Input> {
    // shortcut: a required field the file's `defaults` block supplies still gets TODO, overriding it; upgrade if a
    // builtin ever defaults a required field.
    edit::fields_of(view, kind)
        .into_iter()
        .filter(|field| {
            view.registries()
                .fields()
                .get(kind, &field.name)
                .is_some_and(|entry| entry.declared().required)
        })
        .map(|field| (field.name, Input::One(PLACEHOLDER.to_string())))
        .collect()
}

/// `edit` planned over the navigator's view (ADR 0151), as position edits in the text each file was compiled from;
/// the refusal when the plan is refused (an ID `validate_id` refuses, a taken ID, E080, no project root, a
/// configuration that blocks rewrites) or changes nothing.
fn typed(navigator: &Navigator<'_>, edit: Edit) -> Result<Vec<TextEdit>, Refusal<EditField>> {
    let edits = edit::plan(navigator.view(), std::slice::from_ref(&edit))?.text_edits();
    if edits.is_empty() {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            "not_editable",
            "the fix changes nothing",
        )
        .about(EditField::Entity));
    }
    Ok(edits)
}
