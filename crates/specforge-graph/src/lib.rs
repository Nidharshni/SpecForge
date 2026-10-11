mod build;
mod coerce;
pub use coerce::FieldCoercion;
mod delta;
pub use delta::{EdgeChange, GraphDelta, ModifiedNodeChange, NodeChange, compute_graph_delta};
mod derive;
pub use derive::{DerivedFrom, DerivedReference};
mod graph;
mod obligations;
// The cycle walker lives in specforge-common (the registry's rules use it
// without linking the graph); re-exported here unchanged.
pub use obligations::obligations;
pub use specforge_common::cycles::{CycleOptions, find_cycles};

pub use build::{
    Applied, FileChange, GraphBuild, GraphConfig, GraphInputs, build_graph, build_graph_with_config,
};
pub use graph::{Edge, Graph, Node, Reached};
pub use specforge_common::{Diagnostic, Severity, SourceSpan};
pub use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, SpecFile};
