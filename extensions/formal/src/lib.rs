//! @specforge/formal — the formal-methods builtin, now authored with the
//! SpecForge extension SDK (SDK adoption; spec/extension-sdk docs).
//!
//! Its kinds, edges, enhancements and rules are declared with the SDK
//! builders in [`declaration`], its passes below; the handshake is derived
//! by the SDK from the extension metadata — contribution flags included.

mod conditions;
mod coverage_tracking;
mod declaration;
mod event_graph;
mod layering;

use conditions::{pass_analysis_available, pass_condition_check};
use coverage_tracking::pass_coverage_tracking;
use event_graph::pass_event_graph_analyze;
use layering::pass_layering_verify;
use specforge_extension_sdk::prelude::*;
use specforge_extension_sdk::PassEntity;

#[specforge_extension_sdk::extension(
    name = "@specforge/formal",
    version = "1.0.0",
    description = "Formal methods: properties, axioms, protocols, refinements and processes, with contract, layering and coverage passes"
)]
struct Formal;

impl Contributions for Formal {
    fn contribute(c: &mut ContributionsBuilder) {
        // Diagrams (`model`, `outline`) draw the extension in this colour.
        c.theme_color("#9b59b6");
        c.peer("@specforge/software", "^1.0");

        // Declaration order is deliberately shuffled; the host must order
        // passes by the declared after-constraints, not by declaration.
        c.pass("event_graph_analyze", |p| {
            p.after("layering_verify").run(pass_event_graph_analyze);
        });
        c.pass("coverage_tracking", |p| {
            p.after("event_graph_analyze").run(pass_coverage_tracking);
        });
        c.pass("condition_check", |p| {
            p.after("resolve").run(pass_condition_check);
        });
        c.pass("layering_verify", |p| {
            p.after("condition_check").run(pass_layering_verify);
        });
        // The one check-phase pass: it runs with every compile, the four
        // above only under `specforge analyze`.
        c.pass("analysis_available", |p| {
            p.after("resolve")
                .phase("check")
                .run(pass_analysis_available);
        });

        declaration::declare(c);
    }
}

// ── Extension-owned compiler passes ────────────────────────────────────────
// Each pass is declared with its handler above: `__pass_<name>` receives the
// protocol's PassInput snapshot and answers its diagnostics (ADR 0013).

/// The kind whose entities are coverage items whatever their kind's
/// testability.
const INVARIANT_KIND: &str = "invariant";

fn non_empty(entity: &PassEntity, field: &str) -> bool {
    entity
        .fields
        .get(field)
        .is_some_and(|v| !v.trim().is_empty())
}

const BEHAVIOR_KIND: &str = "behavior";

#[cfg(test)]
mod pass_tests;

#[cfg(test)]
mod coverage_tracking_tests;

specforge_extension_sdk::component_guest!(build = specforge_extension_build);

// -- C10-00/C10-11: process semantics + detector soundness slivers --

#[cfg(test)]
mod process_tests;

// -- ADR 0040: the formal diagnostics the specs name --

#[cfg(test)]
mod formal_diagnostics_tests;
