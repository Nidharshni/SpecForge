//! The graph build's linking step: the one place references resolve, so a
//! compile and every update agree (`GraphBuild`). What a field's value
//! references is its [`Reading`](crate::Reading)'s answer, never the
//! parser's shape (ADR 0093).

use specforge_common::{Diagnostic, DiagnosticData, SourceSpan, Sym, codes};
use std::collections::BTreeMap;

use crate::{Edge, FieldForms, Graph};

mod reference;

/// An edge as written, and whether a declared single reference wrote it.
struct WrittenEdge {
    edge: Edge,
    /// Where the reference is written.
    at: Option<SourceSpan>,
    single: bool,
}

impl Graph {
    /// Resolve every node's references into edges and report what does not
    /// resolve (`Graph::resolve` decides each, ADR 0117): E003 (with a
    /// fuzzy-match suggestion when there is one), or I004 for a field whose
    /// target kind no loaded extension declares, naming the extension in
    /// `owners` that does. Then W061 over the edges but the declared single
    /// references': a single reference is never part of a reference cycle.
    ///
    /// Records where each edge is written (`Graph::written`, ADR 0127): the span of each reference it was linked from.
    ///
    /// Clears all existing edges first. Diagnostics come in node order
    /// (the graph's nodes are sorted), then the cycles'.
    pub(crate) fn link(
        &mut self,
        forms: &FieldForms,
        owners: &BTreeMap<String, String>,
    ) -> Vec<Diagnostic> {
        self.clear_edges();
        let mut diagnostics = Vec::new();
        // Every edge in written order, and whether it is a single reference's.
        let mut edges: Vec<WrittenEdge> = Vec::new();

        for node in self.nodes.values() {
            let kind = node.kind.raw.as_str();
            for entry in node.fields.entries() {
                let reading = forms.reading(kind, entry.key.as_str());
                let single = reading.is_single_reference();
                let target = forms.target(kind, entry.key.as_str());
                for reference in reading.references(entry) {
                    let resolution = self.resolve(target, owners, reference.id);
                    if resolution.links() {
                        edges.push(WrittenEdge {
                            edge: Edge {
                                source: node.id.raw,
                                target: Sym::new(reference.id),
                                label: entry.key,
                            },
                            at: reference.span.cloned(),
                            single,
                        });
                    }
                    diagnostics.extend(resolution.diagnostic(node, entry.key, &reference));
                }
            }
        }

        for written in edges.iter().filter(|w| !w.single) {
            self.add_edge(written.edge);
        }
        diagnostics.extend(self.cycle_warnings());
        // The single references join, keeping the edges in written order.
        self.clear_edges();
        for written in edges {
            self.add_written(written.edge, written.at.as_ref());
        }
        diagnostics
    }

    /// W061 for every reference cycle among the edges added so far.
    fn cycle_warnings(&self) -> Vec<Diagnostic> {
        self.detect_cycles()
            .iter()
            .map(|cycle| {
                let path: Vec<String> = cycle.iter().map(|s| s.to_string()).collect();
                Diagnostic::new(
                    codes::W061,
                    format!("reference cycle detected: {}", path.join(" -> ")),
                )
                .with_suggestion("break the cycle by removing or inverting one reference")
                .with_data(DiagnosticData::ReferenceCycle { path })
            })
            .collect()
    }

    /// The references that name an existing entity but have no edge
    /// (source, target, field). Empty after every link.
    #[cfg_attr(
        not(debug_assertions),
        allow(dead_code, reason = "only debug assertions call it")
    )]
    pub(crate) fn unlinked_references(&self, forms: &FieldForms) -> Vec<(Sym, Sym, Sym)> {
        let mut unlinked = Vec::new();
        for node in self.nodes.values() {
            let source = node.id.raw;
            let edges = self.source_index.get(&source);
            let kind = node.kind.raw.as_str();
            for entry in node.fields.entries() {
                let field_target = forms.target(kind, entry.key.as_str());
                for reference in forms.reading(kind, entry.key.as_str()).references(entry) {
                    if !self
                        .resolve(field_target, &BTreeMap::new(), reference.id)
                        .links()
                    {
                        continue;
                    }
                    let target = Sym::new(reference.id);
                    let linked = edges.is_some_and(|indices| {
                        indices.iter().any(|&i| {
                            let edge = &self.edges[i];
                            edge.target == target && edge.label == entry.key
                        })
                    });
                    if !linked {
                        unlinked.push((source, target, entry.key));
                    }
                }
            }
        }
        unlinked
    }

    /// The edges with no written place (source, target, field). Empty after
    /// every link: each reference and each derived name records its span.
    #[cfg_attr(
        not(debug_assertions),
        allow(dead_code, reason = "only debug assertions call it")
    )]
    pub(crate) fn unwritten_edges(&self) -> Vec<(Sym, Sym, Sym)> {
        self.edges
            .iter()
            .zip(&self.written)
            .filter(|(_, places)| places.is_empty())
            .map(|(edge, _)| (edge.source, edge.target, edge.label))
            .collect()
    }

    /// Panic, naming them, when [`Self::unlinked_references`] or
    /// [`Self::unwritten_edges`] is not empty: a SpecForge bug, never a spec
    /// error.
    #[cfg_attr(
        not(debug_assertions),
        allow(dead_code, reason = "only debug assertions call it")
    )]
    pub(crate) fn assert_linked(&self, forms: &FieldForms) {
        let unlinked = self.unlinked_references(forms);
        assert!(
            unlinked.is_empty(),
            "SpecForge bug: references to existing entities without their edge (source, target, field): {}",
            unlinked
                .iter()
                .map(|(source, target, field)| format!("({source}, {target}, {field})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let unwritten = self.unwritten_edges();
        assert!(
            unwritten.is_empty(),
            "SpecForge bug: edges the linker made with no written place (source, target, field): {}",
            unwritten
                .iter()
                .map(|(source, target, field)| format!("({source}, {target}, {field})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    const REFERENCING: &str = r#"
behavior alpha "A" {
  contract "first"
  invariants [inv_one]
}
invariant inv_one "Invariant One" {
  contract "must hold"
}
"#;

    /// The graph and diagnostics of `source`, built the way a compile builds it.
    fn built(source: &str) -> (Graph, Vec<Diagnostic>) {
        crate::build_graph(&[specforge_parser::parse(source, "main.spec")])
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "a reference to an existing entity without its edge fails the linker's assertion"
    )]
    #[should_panic(expected = "without their edge")]
    fn a_reference_without_its_edge_fails_the_assertion() {
        let (mut graph, _) = built(REFERENCING);
        // What a linker that forgot an edge leaves behind.
        graph.clear_edges();

        graph.assert_linked(&FieldForms::default());
    }

    #[test]
    fn unlinked_references_names_the_missing_edge() {
        let (mut graph, _) = built(REFERENCING);
        graph.clear_edges();

        let unlinked: Vec<(&str, &str, &str)> = graph
            .unlinked_references(&FieldForms::default())
            .iter()
            .map(|(s, t, f)| (s.as_str(), t.as_str(), f.as_str()))
            .collect();

        assert_eq!(unlinked, [("alpha", "inv_one", "invariants")]);
    }

    #[test]
    #[should_panic(expected = "no written place")]
    fn an_edge_without_a_written_place_fails_the_assertion() {
        let (mut graph, _) = built(REFERENCING);
        // What a linker that forgot a span leaves behind.
        graph.clear_edges();
        graph.add_edge(Edge {
            source: Sym::new("alpha"),
            target: Sym::new("inv_one"),
            label: Sym::new("invariants"),
        });

        graph.assert_linked(&FieldForms::default());
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "reference with corresponding graph edge passes"
    )]
    fn a_linked_reference_passes() {
        let (graph, _) = built(REFERENCING);
        graph.assert_linked(&FieldForms::default());
        assert!(graph.unlinked_references(&FieldForms::default()).is_empty());
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "an empty graph passes the linker's assertion"
    )]
    fn an_empty_graph_passes() {
        let (graph, _) = built("");
        assert_eq!(graph.edge_count(), 0);
        graph.assert_linked(&FieldForms::default());
    }

    #[specforge_test(
        behavior = "detect_dangling_references",
        verify = "Detect Dangling References: dangling reference detection holds — graph_built_fired, resolver_integrity_verified, no_duplicate_diagnostics"
    )]
    fn a_missing_target_is_e003_once() {
        // An unresolved id is the linker's E003, once; it names no existing
        // entity, so the assertion has nothing to say.
        let source = "behavior beta \"B\" { contract \"second\" invariants [missing] }\n";
        let (graph, diagnostics) = built(source);
        assert_eq!(diagnostics.iter().filter(|d| d.code == "E003").count(), 1);
        assert!(graph.unlinked_references(&FieldForms::default()).is_empty());
        graph.assert_linked(&FieldForms::default());
    }
}
