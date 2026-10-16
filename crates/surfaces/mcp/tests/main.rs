#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod asking;
mod call_target;
mod conformance;
mod contracts;
mod event_spec;
mod events;
mod failure_kinds;
mod fake_extension;
mod infer_sources;
mod invariants;
mod lifecycle;
mod local_specifier_containment;
mod management_view;
mod message_spec;
mod methods;
mod mutation_outcome;
mod navigation_text;
mod operations_mgmt;
mod operations_mutation;
mod option_tables;
mod output_schemas;
mod path_containment;
mod path_spelling;
mod product_commands;
mod project_file_containment;
mod prompt_golden;
mod prompt_infer;
mod prompt_pins;
mod prompts;
mod protocol;
mod read_notices;
mod refusal_arguments;
mod refusal_wire;
mod request_meaning;
mod resource_templates;
mod resources;
mod revision;
mod schema_reflection;
mod served;
mod served_project;
mod stateless;
mod stdio;
mod subscriptions;
mod support;
mod surface_call;
mod surface_wiring;
mod tool_arguments;
mod tool_dispatch;
mod tool_errors;
mod tool_replies;
mod tools_core;
mod tools_inference;
mod tools_navigation;
