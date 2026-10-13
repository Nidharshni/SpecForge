// Wasm extension lifecycle events — load, init, validate, unload, deps,
// sorting, manifest, integrity, install, remove, upgrade

use "types/wasm"

event extension_loaded "Extension Loaded" {
  channel "wasm.extension_loaded"
  payload ExtensionLoadedPayload
  verify integration "emits extension_loaded with correct extensionName, wasmSize, fromCache, and loadTimeMs"
  verify integration "consumer initialize_wasm_extension receives event and begins initialization"
}

type ExtensionLoadedPayload {
  extensionName string
  wasmSize      integer
  fromCache     boolean
  loadTimeMs    integer
  verify unit "ExtensionLoadedPayload schema is valid"
}

event extension_initialized "Extension Initialized" {
  channel "wasm.extension_initialized"
  payload ExtensionInitializedPayload
  verify integration "emits extension_initialized with correct entityCount and edgeTypeCount"
}

type ExtensionInitializedPayload {
  extensionName string
  entityCount   integer
  edgeTypeCount integer
  verify unit "ExtensionInitializedPayload schema is valid"
}

event extension_unloaded "Extension Unloaded" {
  channel "wasm.extension_unloaded"
  payload ExtensionUnloadedPayload
  verify integration "emits extension_unloaded with correct extensionName and reason"
}

type ExtensionUnloadedPayload {
  extensionName string
  reason        string
  verify unit "ExtensionUnloadedPayload schema is valid"
}

// ── Runtime Observability Events ───────────────────────────────

event manifest_validated "Manifest Validated" {
  channel "wasm.manifest_validated"
  payload ManifestValidatedPayload
  verify integration "emits manifest_validated with correct entityKindCount and validation status"
}

type ManifestValidatedPayload {
  extensionName    string
  protocolVersion  string
  entityKindCount  integer
  validationPassed boolean
  verify unit "ManifestValidatedPayload schema is valid"
}

// ── Extension Lifecycle Events ─────────────────────────────────

event extension_install_completed "Extension Install Completed" {
  channel "wasm.extension_install_completed"
  payload ExtensionInstallCompletedPayload
  verify integration "emits extension_install_completed with correct extensionName, version, source, and installTimeMs"
  verify integration "consumer load_wasm_module receives event after install"
}

type ExtensionInstallCompletedPayload {
  extensionName string
  version       string
  source        string
  installTimeMs integer
  verify unit "ExtensionInstallCompletedPayload schema is valid"
}

event wasm_extension_removed "Wasm Extension Removed" {
  channel "wasm.extension_removed"
  payload WasmExtensionRemovedPayload
  verify integration "emits extension_removed with correct extensionName"
  verify integration "consumer reuse_session_runtime drops the extension's loaded instance"
}

type WasmExtensionRemovedPayload {
  extensionName string
  verify unit "WasmExtensionRemovedPayload schema is valid"
}

event extension_upgrade_completed "Extension Upgrade Completed" {
  channel "wasm.extension_upgrade_completed"
  payload ExtensionUpgradeCompletedPayload
  // One per extension an applied `specforge update` moved: an upgrade whose
  // peer check fails is refused and completes nothing, so the payload
  // carries no peer verdict. `specforge update --format json` reports them
  // under `extension_upgrade_completed`; an update that was not applied
  // reports none.
  verify integration "emits extension_upgrade_completed with correct previousVersion and newVersion"
  verify integration "consumer load_wasm_module receives event after upgrade"
}

type ExtensionUpgradeCompletedPayload {
  extensionName   string
  previousVersion string
  newVersion      string
  verify unit "ExtensionUpgradeCompletedPayload schema is valid"
}
