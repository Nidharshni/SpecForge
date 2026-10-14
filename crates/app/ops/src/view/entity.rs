//! An entity a request names that the project view lacks: the one refusal for it (ADR 0015 Q5) and
//! the request field a read view's refusal is about (ADR 0072).

use specforge_common::{codes, find_close_match};
use specforge_graph::Graph;

use crate::{OpError, OpErrorKind};

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
