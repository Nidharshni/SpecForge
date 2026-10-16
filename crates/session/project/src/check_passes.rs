//! Check-phase extension passes: compiler passes an extension declares with
//! `phase: "check"` run with every compile, after the graph checks, and
//! what they report is the compile's (behavior `run_check_phase_passes`).
//! They run through the one pass loop
//! ([`crate::passes::run_extension_passes`]); a pass's summary is not read.

use crate::passes::{PassContext, run_extension_passes};
use specforge_common::Diagnostic;
use specforge_registry::Disk;
use specforge_registry::EntitySnapshot;
use specforge_wasm::WasmRuntime;

use crate::Environment;
use crate::build_cache::BuildCache;

/// Run `env`'s check passes over the entity snapshot
/// `entities`, handing them the build cache as `previous` when the
/// project has one (W144 when it is invalid). A pass that fails (it traps,
/// or answers output that does not parse) is E028; the others still run.
pub(crate) fn run(
    env: &Environment,
    entities: &EntitySnapshot,
    runtime: &dyn WasmRuntime,
    disk: &dyn Disk,
) -> Vec<Diagnostic> {
    if env.registries().check_passes().next().is_none() {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();
    // The build cache is the check passes' declared input: read on every
    // compile that runs one, so a cache just written is seen.
    let previous = match BuildCache::read(disk, env.root()) {
        Ok(previous) => previous,
        Err(invalid) => {
            diagnostics.push(invalid);
            None
        }
    };
    let context = PassContext {
        previous: previous.as_ref(),
        ..PassContext::of(entities)
    };
    diagnostics.extend(
        run_extension_passes(env.registries().check_passes(), &context, runtime)
            .into_iter()
            .flat_map(|report| report.findings),
    );
    diagnostics
}
