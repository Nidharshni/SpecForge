#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod budget_scope;
mod contracts;
mod determinism;
mod emit_brief;
mod emit_context;
mod emit_dot;
mod emit_graph;
mod emit_json;
mod emit_markdown;
mod emit_matrix;
mod emit_members;
mod emitter_error;
mod model;
mod outline;
mod outline_owner;
mod schema;
mod scope;
mod stress;
mod support;
mod token_budget;

/// `graph` exported as `format`, unscoped and unbudgeted, read with `registries` (the empty build
/// when `None`): what `emit` answers for it.
pub(crate) fn export(
    graph: &specforge_graph::Graph,
    format: specforge_emitter::EmitFormat,
    registries: Option<&specforge_registry::RegistryBuild>,
) -> String {
    specforge_emitter::emit(
        graph,
        &specforge_emitter::EmitOptions {
            format,
            registries: registries.unwrap_or(specforge_emitter::EmitOptions::default().registries),
            ..specforge_emitter::EmitOptions::default()
        },
    )
    .expect("an unscoped, unbudgeted export cannot fail")
}
