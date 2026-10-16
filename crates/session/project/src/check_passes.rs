//! Check-phase extension passes: compiler passes an extension declares with
//! `phase: "check"` run with every compile, after the graph checks, and
//! what they report is the compile's (behavior `run_check_phase_passes`).

use crate::passes::{PassContext, pass_findings, pass_input};
use specforge_common::Diagnostic;
use specforge_registry::Disk;
use specforge_registry::EntitySnapshot;
use specforge_wasm::{ExtensionCalls, WasmRuntime};

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
    let input = pass_input(&PassContext {
        previous: previous.as_ref(),
        ..PassContext::of(entities)
    });
    let encoded = ExtensionCalls::encode(&input);
    let calls = ExtensionCalls::new(runtime);
    for declared in env.registries().check_passes() {
        let pass = &declared.pass;
        let answer = calls.run_pass(&declared.extension, &pass.name, &encoded);
        match answer {
            Ok(output) => diagnostics.extend(pass_findings(
                &declared.extension,
                &pass.name,
                output,
                entities,
            )),
            Err(error) => diagnostics.push(error.diagnostic()),
        }
    }
    diagnostics
}
