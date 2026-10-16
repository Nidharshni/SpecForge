//! The graph views and project files operations compute: traces, plan
//! validation, stats.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod compiled_support;
mod connectivity;
mod contracts;
mod contracts_pass;
mod coverage;
mod cross_extension;
mod determinism;
mod diagnostics_json;
mod enhancement_credit;
mod errors;
mod explore;
mod export_cache;
mod extension_calls;
mod failures;
mod field_references;
mod field_types;
mod formal_diagnostics;
mod graph_edges;
mod groups;
mod infer;
mod infer_guide;
mod init_specifiers;
mod init_writes;
mod inspect;
mod kinds;
mod management;
mod migrate;
mod migrate_sources;
mod model;
mod multi_entity;
mod navigate;
mod obligations_member;
mod options;
mod peers;
mod plan;
mod project_files;
mod publish;
mod query;
mod review;
mod schema;
mod schema_notices;
mod setup;
mod stats;
mod trace;
mod trace_support;
mod verify_statements;
mod view_support;
mod writes;
