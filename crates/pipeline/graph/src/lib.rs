//! The typed entity graph. A `GraphBuild` turns parsed `.spec` files into a `Graph` of nodes and edges and reports
//! what building it found (duplicates, unknown ref schemes, unresolved references, reference cycles), kept current
//! one whole file at a time (ADR 0032). `compute_graph_delta` says what changed between two graphs, `FieldForm` how
//! a declared field is written, `FileDefaults` what a file's `defaults` blocks give its blocks, `written_tokens` which
//! tokens of a file name an entity, and `Obligation` how an entity's `verify` statements are written.

mod build;
mod defaults;
pub use defaults::{FileDefaults, Held};
pub mod field_form;
pub use field_form::{
    FieldForm, FieldForms, FieldJson, Reading, Reference, Rewrite, Target, ValueShape,
};
mod delta;
pub use delta::{
    EdgeChange, GraphDelta, ModifiedNodeChange, NodeAspect, NodeChange, compute_graph_delta,
    moved_nodes,
};
mod derive;
pub use derive::{DerivedFrom, DerivedReference};
mod graph;
mod obligations;
mod tokens;
pub use obligations::Obligation;
pub use tokens::{Form, Holder, WrittenToken, written_tokens};

pub use build::{Applied, FileChange, GraphBuild, GraphConfig, GraphInputs};
#[cfg(any(test, feature = "testing"))]
pub use build::{build_graph, build_graph_with_config};
pub use graph::{Degree, Direction, Edge, Graph, Node, Reached, Selected};
pub use specforge_common::{Diagnostic, Severity, SourceSpan};
pub use specforge_parser::{EntityId, EntityKind, FieldMap, FieldValue, SpecFile, Written};
