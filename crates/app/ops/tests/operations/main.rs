//! The graph views and project files operations compute: traces, plan
//! validation, stats.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod anchors;
mod collect;
mod compiled_support;
mod connectivity;
mod contracts;
mod contracts_pass;
mod coverage;
mod cross_extension;
mod defaults;
mod determinism;
mod diagnostics_json;
mod edit;
mod enhancement_credit;
mod entity_arguments;
mod errors;
mod explore;
mod export_cache;
mod export_written;
mod extension_calls;
mod failed_run;
mod failures;
mod field_references;
mod field_types;
mod finder_references;
mod fix_edits;
mod fixes;
mod formal_diagnostics;
mod graph_edges;
mod groups;
mod infer;
mod infer_guide;
mod infer_sources;
mod init_specifiers;
mod init_writes;
mod inspect;
mod kinds;
mod local_component;
mod machine_reach;
mod management;
mod migrate;
#[cfg(unix)]
mod migrate_bound;
#[cfg(unix)]
mod migrate_change;
mod migrate_sources;
mod migrate_titles;
mod model;
mod multi_entity;
mod navigate;
mod obligations_member;
mod options;
mod peers;
mod plan;
mod previews;
mod project_files;
mod publish;
mod query;
mod refusals;
mod registry_search;
mod remedy_wording;
mod rename_refusals;
mod rename_text;
mod review;
mod schema;
mod schema_notices;
mod search;
mod setup;
mod stats;
mod trace;
mod trace_order;
mod trace_support;
mod verify_statements;
mod view_support;
mod writes;
mod written_references;
mod written_tokens;
