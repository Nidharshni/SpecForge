//! Export formats: a built graph, and the registries that describe it, as
//! text. The graph export (`json`, Graph Protocol V2 in `schema`), the
//! agent formats (`context`, `brief`, scoped and token-budgeted through
//! [`emit`]), DOT, and the schema-level `model` and extension `outline`
//! diagrams. Pure: no compile, no Wasm runtime, no file I/O (ADR 0004
//! D1-d). Compiling a project is `specforge-project`'s; traces, plans,
//! stats and the schema cache are `specforge-ops`'.

mod brief;
mod budget;
pub use budget::estimate_tokens;
pub mod context;
mod diagram;
pub mod dot;
mod emit;
mod error;
pub mod json;
pub mod model;
pub mod outline;
mod publish;
pub mod schema;

pub use dot::DotOptions;
pub use emit::{EmitFormat, EmitOptions, document_schema, emit};
pub use error::EmitterError;
pub use json::SCHEMA_VERSION;
pub use publish::publish;
pub use schema::{
    GraphProtocolSchema, SchemaCacheEntry, SchemaCompatibility, SchemaEdgeType, SchemaEntityKind,
    SchemaExtensionInfo, SchemaField, SchemaMigration, SchemaMigrationChange, SchemaVersion,
    SchemaVersionError, compute_schema_version, content_hash, diff_schemas, diff_schemas_optional,
    generate_schema, negotiate_version,
};
