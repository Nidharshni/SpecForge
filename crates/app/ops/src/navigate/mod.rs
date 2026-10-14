//! Navigating a project: where an entity is declared, what references it,
//! which entities match a text, which entities a diagnostic is about, and
//! the edits that fix a diagnostic. The LSP and MCP answer from here; they
//! convert spans (UTF-16 ranges, JSON) and nothing else. Positions are
//! `SourceSpan`s: 1-based lines, 1-based byte columns, end exclusive.
//!
//! A reference is an occurrence of an entity's ID in another entity's
//! field that resolves to it: one edge of the graph, written where its
//! token is. The references *to* an entity are incoming; what it *refers
//! to* are its outgoing references. Navigation names no kind: kinds,
//! fields and their targets come from the registries (ADR 0016).

mod anchors;
mod attribution;
mod files;
mod find;
mod fixes;
mod occurrences;
mod references;
mod text;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use specforge_common::{SourceSpan, Sym, codes, find_close_match};
use specforge_graph::Graph;

use crate::view::ProjectView;
use crate::{OpError, OpErrorKind};

pub use anchors::{
    ANCHORS_FILENAME, AnchorManifest, SourceAnchor, anchors_of_entity, source_anchors,
};
pub use attribution::{is_about, subjects};
pub use files::{
    FileAnchors, FileMatch, OutlineEntry, OutlineMethod, anchors_of_file, match_file, outline,
};
pub use find::{
    EntityMatch, EntityQuery, FUZZY_THRESHOLD, MatchScope, MatchedOn, Tier, find_entities, snippet,
    within_fuzzy_threshold,
};
pub use fixes::{Fix, FixKind, FixQuery, FixSource, TextEdit};
pub use occurrences::{
    DIRECTION, Definition, Direction, Occurrence, Precision, ReferenceQuery, Role,
};
pub use references::{Reference, References};

use text::SourceText;

/// What navigation reads: the project view and each spec file's text (an
/// open buffer first for the LSP; the file under the spec root for MCP),
/// by its key relative to the spec root, as spans name it. Each file is
/// read at most once per navigator.
pub struct Navigator<'a, F: Fn(&str) -> Option<String>> {
    view: ProjectView<'a>,
    text_of: F,
    texts: RefCell<HashMap<Sym, Option<Rc<SourceText>>>>,
}

impl<'a, F: Fn(&str) -> Option<String>> Navigator<'a, F> {
    pub fn new(view: ProjectView<'a>, text_of: F) -> Self {
        Navigator {
            view,
            text_of,
            texts: RefCell::new(HashMap::new()),
        }
    }

    /// The project view navigation reads.
    pub fn view(&self) -> &ProjectView<'a> {
        &self.view
    }

    /// The text of `file`, read once.
    fn text(&self, file: Sym) -> Option<Rc<SourceText>> {
        if let Some(text) = self.texts.borrow().get(&file) {
            return text.clone();
        }
        let text = (self.text_of)(file.as_str()).map(|t| Rc::new(SourceText::new(t)));
        self.texts.borrow_mut().insert(file, text.clone());
        text
    }

    /// The entity `id`, or [`not_found`].
    fn node(&self, id: &str) -> Result<&'a specforge_graph::Node, OpError> {
        let graph = self.view.graph();
        graph.node(id).ok_or_else(|| not_found(graph, id))
    }
}

/// The field of a read view's request a refusal can be about (ADR 0072): the one entity the
/// view is asked about (inspect, definition, references, neighbourhood, explore, review, trace).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityField {
    /// The entity the view is about.
    EntityId,
}

/// A request names an entity the graph lacks: E003, of kind
/// `EntityNotFound`, about `id`, worded `unresolved entity '<id>' - not
/// found in graph`, with `did you mean '<closest>'?` when an id of `graph`
/// is close. The one refusal for it: export's scope, inspect, the
/// navigator, rename, trace, and every MCP tool and prompt.
pub fn not_found(graph: &Graph, id: &str) -> OpError {
    unresolved(
        id,
        find_close_match(id, graph.nodes().iter().map(|n| n.id.raw.as_str())),
    )
}

/// [`not_found`] with the closest id already known (a `TraceError`).
pub(crate) fn unresolved(id: &str, near: Option<&str>) -> OpError {
    let error = OpError::coded(
        OpErrorKind::EntityNotFound,
        codes::E003,
        format!("unresolved entity '{id}' — not found in graph"),
    )
    .with_entity(id);
    match near {
        Some(near) => error.with_suggestion(format!("did you mean '{near}'?")),
        None => error,
    }
}

/// Whether span `inner` lies within `outer`: same file, and its start and
/// end are between `outer`'s, comparing (line, column).
pub fn contains(outer: &SourceSpan, inner: &SourceSpan) -> bool {
    outer.file == inner.file
        && (outer.start_line, outer.start_col) <= (inner.start_line, inner.start_col)
        && (inner.end_line, inner.end_col) <= (outer.end_line, outer.end_col)
}

/// Whether spans `a` and `b` share a position (touching counts): same
/// file, neither ends before the other starts.
pub fn overlaps(a: &SourceSpan, b: &SourceSpan) -> bool {
    a.file == b.file
        && (a.start_line, a.start_col) <= (b.end_line, b.end_col)
        && (b.start_line, b.start_col) <= (a.end_line, a.end_col)
}
