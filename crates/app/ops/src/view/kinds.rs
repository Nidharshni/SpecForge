//! The entity kinds a project knows, the one answer to "is this a known
//! kind" (ADR 0015, section "Query"), and the one reading of a kinds list
//! written as text (ADR 0220).

use std::collections::{BTreeMap, BTreeSet};

use specforge_common::{Diagnostic, codes, find_close_match};

use super::ProjectView;
use crate::{OpError, OpErrorKind};

/// What an `unknown_kind` refusal is reported as (ADR 0015 D8).
pub const UNKNOWN_KIND: &str = "unknown_kind";

/// The entity kinds a project knows: the kinds its loaded extensions
/// declare (the registry build's keywords), and the kinds its entities are
/// written with (a kind no extension declares is E024's, and still the
/// kind of an entity). Kind names are keywords: matched exactly, case
/// included. The closest kind to an unknown one is a kind equal to it
/// ignoring case, else the closest by edit distance
/// ([`find_close_match`]).
pub struct KnownKinds<'a> {
    declared: BTreeSet<&'a str>,
    written: BTreeSet<&'a str>,
}

impl<'a> ProjectView<'a> {
    /// The kinds this view's project knows.
    pub fn kinds(&self) -> KnownKinds<'a> {
        let declared: BTreeSet<&'a str> = self
            .registries()
            .kinds()
            .keywords()
            .map(String::as_str)
            .collect();
        let written = self
            .graph()
            .nodes()
            .into_iter()
            .map(|node| node.kind.raw.as_str())
            .filter(|kind| !declared.contains(kind))
            .collect();
        KnownKinds { declared, written }
    }
}

/// The kinds a read keeps, as given, and what they noticed (ADR 0015 Q3): the kinds the project
/// lacks, as I020. Which entities they keep is the graph's ([`specforge_graph::Graph::select`]).
pub struct KindFilter<'r> {
    kinds: &'r [&'r str],
    notices: Vec<Diagnostic>,
}

impl<'r> KindFilter<'r> {
    /// The filter as given, unknown kinds included.
    pub fn kinds(&self) -> &'r [&'r str] {
        self.kinds
    }

    /// I020 for each kind the project lacks, once each, in the filter's order.
    pub fn notices(&self) -> &[Diagnostic] {
        &self.notices
    }

    /// [`Self::notices`], owned.
    pub fn into_notices(self) -> Vec<Diagnostic> {
        self.notices
    }
}

/// The kinds a kinds list written as text names (ADR 0220): the items between its commas, in order, each
/// without the whitespace around it. An empty item stays: it is the empty kind, which a kind filter reports as
/// I020 and which keeps nothing (ADR 0177 S5). A list that arrives as a list (an MCP tool's `kinds`) is not
/// text: its items are the kinds as given. A kind is an identifier (the grammar's `kind`), so the trim drops
/// nothing a kind could hold.
pub fn kinds_of_text(text: &str) -> impl Iterator<Item = &str> {
    text.split(',').map(str::trim)
}

/// [`kinds_of_text`] over each of several texts (a repeatable `--kinds` flag), in order.
pub fn kinds_of_texts(texts: &[String]) -> impl Iterator<Item = &str> {
    texts.iter().flat_map(|text| kinds_of_text(text))
}

impl ProjectView<'_> {
    /// The filter `kinds` (empty: every kind), with I020 for each kind the project lacks.
    pub fn kind_filter<'r>(&self, kinds: &'r [&'r str]) -> KindFilter<'r> {
        KindFilter {
            kinds,
            notices: self.kinds().unknown_in(kinds),
        }
    }

    /// How many entities each kind has, for every kind an entity is written
    /// with (an undeclared one, E024's, included), in kind order.
    pub fn entities_by_kind(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for node in self.graph().nodes() {
            *counts.entry(node.kind.raw.as_str()).or_insert(0) += 1;
        }
        counts
    }
}

impl KnownKinds<'_> {
    /// A loaded extension declares `kind`.
    pub fn declares(&self, kind: &str) -> bool {
        self.declared.contains(kind)
    }

    /// An entity can be of `kind`: declared, or written by an entity.
    pub fn has(&self, kind: &str) -> bool {
        self.declares(kind) || self.written.contains(kind)
    }

    /// The kinds of a filter over entities that the project does not have
    /// ([`has`](Self::has) is false), once each, in the filter's order, as
    /// I020 notices `unknown entity kind '<kind>'` with `did you mean
    /// '<closest>'?` among every kind it has. The filter still drops them.
    fn unknown_in(&self, filter: &[&str]) -> Vec<Diagnostic> {
        let mut reported: Vec<&str> = Vec::new();
        let mut notices = Vec::new();
        for &kind in filter {
            if self.has(kind) || reported.contains(&kind) {
                continue;
            }
            reported.push(kind);
            let mut notice = Diagnostic::new(codes::I020, format!("unknown entity kind '{kind}'"));
            if let Some(close) = closest(kind, self.declared.iter().chain(&self.written).copied()) {
                notice = notice.with_suggestion(format!("did you mean '{close}'?"));
            }
            notices.push(notice);
        }
        notices
    }

    /// `kind` as an argument that needs its declaration (a schema entry,
    /// an inference guide): `Ok` when declared, else `unknown_kind`
    /// (`InvalidInput`), `unknown entity kind '<kind>'`, with `did you mean
    /// '<closest>'?` among the declared kinds.
    pub fn declared(&self, kind: &str) -> Result<(), OpError> {
        if self.declares(kind) {
            return Ok(());
        }
        let error = OpError::new(
            OpErrorKind::InvalidInput,
            UNKNOWN_KIND,
            format!("unknown entity kind '{kind}'"),
        );
        Err(match closest(kind, self.declared.iter().copied()) {
            Some(close) => error.with_suggestion(format!("did you mean '{close}'?")),
            None => error,
        })
    }
}

/// The kind of `among` closest to `kind`: one equal to it ignoring case
/// (the smallest, when several), else the closest by edit distance.
fn closest<'a>(kind: &str, among: impl Iterator<Item = &'a str> + Clone) -> Option<&'a str> {
    let same_but_for_case = among
        .clone()
        .filter(|candidate| candidate.eq_ignore_ascii_case(kind))
        .min();
    same_but_for_case.or_else(|| find_close_match(kind, among))
}
