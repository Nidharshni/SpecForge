//! The typed entity graph. A `GraphBuild` turns parsed `.spec` files into a `Graph` of nodes and edges and reports
//! what building it found (duplicates, unknown ref schemes, unresolved references, reference cycles), kept current
//! one whole file at a time (ADR 0032). `compute_graph_delta` says what changed between two graphs, `FieldForm` how
//! a declared field is written, and `Obligation` how an entity's `verify` statements are written.

mod build;
pub mod field_form;
pub use field_form::{FieldForm, FieldForms, FieldJson, Reading, Reference, ValueShape};
mod delta;
pub use delta::{
    EdgeChange, GraphDelta, ModifiedNodeChange, NodeAspect, NodeChange, compute_graph_delta,
    moved_nodes,
};
mod derive;
pub use derive::{DerivedFrom, DerivedReference};
mod graph;
mod link;
mod obligations;
// The cycle walker lives in specforge-common (the registry's rules use it
// without linking the graph); re-exported here unchanged.
pub use obligations::Obligation;
pub use specforge_common::cycles::{CycleOptions, find_cycles};

pub use build::{
    Applied, FileChange, GraphBuild, GraphConfig, GraphInputs, build_graph, build_graph_with_config,
};
pub use graph::{Edge, Graph, Node, Reached};
pub use specforge_common::{Diagnostic, Severity, SourceSpan};
pub use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, SpecFile};
