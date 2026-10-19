//! Navigating a project: where an entity is declared, what references it,
//! which entities match a text, and which entities a
//! diagnostic is about. The LSP and MCP answer from here; they
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
mod edits;
mod files;
mod find;
mod occurrences;
mod references;
mod text;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use specforge_common::{SourceSpan, Sym};

use crate::OpError;
use crate::view::{ProjectView, not_found};

pub use anchors::{
    AnchoredEntity, EntityAnchors, FileAnchors, Implementation, anchors_of_entity, anchors_of_file,
};
pub use attribution::{is_about, subjects};
pub use edits::{TextEdit, apply_edits};
pub use files::{FileMatch, OutlineEntry, OutlineMethod, match_file, outline};
pub use find::{
    EntityMatch, EntityQuery, FUZZY_THRESHOLD, MatchScope, MatchedOn, Tier, find_entities, snippet,
    within_fuzzy_threshold,
};
pub use occurrences::{
    DIRECTION, Definition, Direction, Occurrence, Precision, ReferenceQuery, Role, Token,
};
pub use references::{Reference, References};

pub(crate) use text::SourceText;

/// What navigation reads: the project view, and each spec file's text as the
/// view's compiled project read it (`CompiledProject::source_text`): the text
/// its spans are positions in, by the key spans name it (relative to the spec
/// root). Each file's text is indexed once per navigator. A file the compile
/// holds no text of has none: its occurrences fall back to the holder's block
/// (`Precision::Entity`). No surface hands it another text (ADR 0116).
pub struct Navigator<'a> {
    view: ProjectView<'a>,
    texts: RefCell<HashMap<Sym, Option<Rc<SourceText>>>>,
}

impl<'a> Navigator<'a> {
    pub fn new(view: ProjectView<'a>) -> Self {
        Navigator {
            view,
            texts: RefCell::new(HashMap::new()),
        }
    }

    /// The project view navigation reads.
    pub fn view(&self) -> &ProjectView<'a> {
        &self.view
    }

    /// The text of `file` as the compile read it, indexed once.
    pub(crate) fn text(&self, file: Sym) -> Option<Rc<SourceText>> {
        if let Some(text) = self.texts.borrow().get(&file) {
            return text.clone();
        }
        let text = self
            .view
            .project()
            .source_text(file.as_str())
            .map(|t| Rc::new(SourceText::new(t)));
        self.texts.borrow_mut().insert(file, text.clone());
        text
    }

    /// The text of `file` a rename plan keeps: the text its edits are positions in.
    pub(crate) fn source_text(&self, file: &str) -> Option<std::sync::Arc<str>> {
        self.view.project().source_text(file)
    }

    /// The entity `id`, or [`not_found`].
    fn node(&self, id: &str) -> Result<&'a specforge_graph::Node, OpError> {
        let graph = self.view.graph();
        graph.node(id).ok_or_else(|| not_found(graph, id))
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
