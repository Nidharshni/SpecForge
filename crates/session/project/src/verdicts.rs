//! The rule set's custom verdicts from the extensions' modules (ADR 0020
//! D3): one adapter of [`CustomVerdicts`] serves every `check: "custom"`
//! rule of a check, since the extension arrives in the call.

use specforge_protocol_types::ValidatorVerdict;
use specforge_registry::rules::{CustomCall, CustomVerdicts, Subject, Verdict, VerdictError};
use specforge_wasm::{ExtensionCalls, WasmRuntime};

use crate::snapshot::EntitySnapshot;

/// Custom verdicts from the extension that declared each rule, called
/// through [`ExtensionCalls::validate`] (ADR 0013) with the protocol's
/// `ValidatorContext`: an entity's from the snapshot
/// ([`EntitySnapshot::validator_context`]), or the probe's
/// ([`EntitySnapshot::probe_context`]). The guest answers a
/// `ValidatorVerdict`; a failed call is [`VerdictError::Failed`].
pub struct WasmVerdicts<'a> {
    runtime: &'a dyn WasmRuntime,
    /// The entities a check asks about; `None` for the load-time probe.
    entities: Option<&'a EntitySnapshot>,
}

impl<'a> WasmVerdicts<'a> {
    /// Verdicts on `entities`' records, and on the probe.
    pub fn new(runtime: &'a dyn WasmRuntime, entities: &'a EntitySnapshot) -> Self {
        WasmVerdicts {
            runtime,
            entities: Some(entities),
        }
    }

    /// Verdicts on the load-time probe only (no project is checked yet).
    pub fn probe_only(runtime: &'a dyn WasmRuntime) -> Self {
        WasmVerdicts {
            runtime,
            entities: None,
        }
    }
}

impl CustomVerdicts for WasmVerdicts<'_> {
    fn verdict(&self, call: CustomCall<'_>) -> Result<Verdict, VerdictError> {
        let context = match call.subject {
            Subject::Entity(record) => self
                .entities
                .and_then(|entities| entities.validator_context(&record.id))
                .ok_or_else(|| VerdictError::Failed(format!("unknown entity '{}'", record.id)))?,
            Subject::Probe { kind } => EntitySnapshot::probe_context(kind.unwrap_or_default()),
        };
        let verdict = ExtensionCalls::new(self.runtime)
            .validate(call.extension, call.function, &context)
            .map_err(|error| VerdictError::Failed(error.to_string()))?;
        Ok(match verdict {
            ValidatorVerdict::Pass => Verdict::Pass,
            ValidatorVerdict::Fail { field, value } => Verdict::Fail { field, value },
        })
    }
}
