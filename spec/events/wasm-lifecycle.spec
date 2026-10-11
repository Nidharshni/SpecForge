// Wasm extension lifecycle events — load, init, validate, unload, deps,
// sorting, manifest, integrity, install, remove, upgrade

use "types/wasm"

event extension_loaded "Extension Loaded" {
  channel "wasm.extension_loaded"
  payload {
    extensionName string
    wasmSize      integer
    fromCache     boolean
    loadTimeMs    integer
  }
  verify integration "emits extension_loaded with correct extensionName, wasmSize, fromCache, and loadTimeMs"
  verify integration "consumer initialize_wasm_extension receives event and begins initialization"
}

event extension_initialized "Extension Initialized" {
  channel "wasm.extension_initialized"
  payload {
    extensionName string
    entityCount   integer
    edgeTypeCount integer
  }
  verify integration "emits extension_initialized with correct entityCount and edgeTypeCount"
}

event extension_unloaded "Extension Unloaded" {
  channel "wasm.extension_unloaded"
  payload {
    extensionName string
    reason        string
  }
  verify integration "emits extension_unloaded with correct extensionName and reason"
}

// ── Runtime Observability Events ───────────────────────────────

event manifest_validated "Manifest Validated" {
  channel "wasm.manifest_validated"
  payload {
    extensionName    string
    protocolVersion  string
    entityKindCount  integer
    validationPassed boolean
  }
  verify integration "emits manifest_validated with correct entityKindCount and validation status"
}

// ── Extension Lifecycle Events ─────────────────────────────────

event extension_install_completed "Extension Install Completed" {
  channel "wasm.extension_install_completed"
  payload {
    extensionName string
    version       string
    source        string
    installTimeMs integer
  }
  verify integration "emits extension_install_completed with correct extensionName, version, source, and installTimeMs"
  verify integration "consumer load_wasm_module receives event after install"
}

event wasm_extension_removed "Wasm Extension Removed" {
  channel "wasm.extension_removed"
  payload {
    extensionName string
  }
  verify integration "emits extension_removed with correct extensionName"
  verify integration "consumer reuse_session_runtime drops the extension's loaded instance"
}

event extension_upgrade_completed "Extension Upgrade Completed" {
  channel "wasm.extension_upgrade_completed"
  payload {
    extensionName   string
    previousVersion string
    newVersion      string
  }
  // One per extension an applied `specforge update` moved: an upgrade whose
  // peer check fails is refused and completes nothing, so the payload
  // carries no peer verdict. `specforge update --format json` reports them
  // under `extension_upgrade_completed`; an update that was not applied
  // reports none.
  verify integration "emits extension_upgrade_completed with correct previousVersion and newVersion"
  verify integration "consumer load_wasm_module receives event after upgrade"
}
