#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod assembled;
mod buffers;
mod builtin_vocabulary;
mod builtins;
mod changes;
mod check_passes;
mod check_reads;
mod compile;
mod copy;
mod custom_rules;
mod delta;
mod derived_references;
mod disk_changed;
mod dual_mode;
mod e2e_pipeline;
mod edge_types;
mod entity_snapshot;
mod environment;
mod environment_reads;
mod extension_calls;
mod field_types;
mod fixture;
mod link_identity;
mod one_side;
mod policy;
mod product_rules;
mod providers;
mod registered_fields;
mod registry_build_snapshot;
mod registry_fields;
mod registry_order;
mod reopen;
mod rooted;
mod rules;
mod session;
mod source_hops;
mod structural_checks;
mod symlinked_directory;
mod update_bracket;
