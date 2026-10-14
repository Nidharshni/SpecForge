mod contracts;
mod determinism;
mod emit_brief;
mod emit_context;
mod emit_dot;
mod emit_graph;
mod emit_json;
mod emit_matrix;
mod emitter_error;
mod model;
mod outline;
mod schema;
mod scope;
mod stress;
mod support;
mod token_budget;

/// `graph` exported as `format`, unscoped and unbudgeted, with the field
/// registry that declares the context format's headline and normative
/// fields when given: what `emit` answers for it.
pub(crate) fn export(
    graph: &specforge_graph::Graph,
    format: specforge_emitter::EmitFormat,
    fields: Option<&specforge_registry::FieldRegistry>,
) -> String {
    specforge_emitter::emit(
        graph,
        &specforge_emitter::EmitOptions {
            format,
            field_registry: fields,
            ..specforge_emitter::EmitOptions::default()
        },
    )
    .expect("an unscoped, unbudgeted export cannot fail")
}
