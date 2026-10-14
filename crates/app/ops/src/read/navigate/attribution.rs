//! Which entities a diagnostic is about: what its data names, else the
//! entity whose block holds it. Never its message, which is presentation
//! (CONTEXT.md "Diagnostic data").

use specforge_common::Diagnostic;
use specforge_graph::{Graph, Node};

use super::contains;

/// The entities `diagnostic` is about: the ones its data names
/// ([`specforge_common::DiagnosticData::entities`]) that the graph holds,
/// in the data's order; else the innermost entity whose block holds its
/// span (file, line and column); else none (a project-level diagnostic).
pub fn subjects<'g>(graph: &'g Graph, diagnostic: &Diagnostic) -> Vec<&'g Node> {
    if let Some(data) = &diagnostic.data {
        let named: Vec<&Node> = data
            .entities()
            .into_iter()
            .filter_map(|id| graph.node(id))
            .collect();
        if !named.is_empty() {
            return named;
        }
    }
    let Some(span) = &diagnostic.span else {
        return Vec::new();
    };
    graph
        .nodes_in_file(span.file.as_str())
        .into_iter()
        .filter(|node| contains(&node.source_span, span))
        // Innermost: the block that starts last, then ends first.
        .max_by(|a, b| {
            let (a, b) = (&a.source_span, &b.source_span);
            (a.start_line, a.start_col)
                .cmp(&(b.start_line, b.start_col))
                .then((b.end_line, b.end_col).cmp(&(a.end_line, a.end_col)))
                .then(a.file.cmp(&b.file))
        })
        .into_iter()
        .collect()
}

/// Whether `diagnostic` is about the entity `id` ([`subjects`]).
pub fn is_about(graph: &Graph, diagnostic: &Diagnostic, id: &str) -> bool {
    subjects(graph, diagnostic)
        .iter()
        .any(|node| node.id.raw == id)
}
