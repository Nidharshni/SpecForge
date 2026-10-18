// Extension development workflow — scaffold, build, test, publish

use "events/wasm-authoring"
use "invariants/extensions"
use "invariants/wasm"
use "ports/outbound"
use "types/errors"
use "types/wasm"

defaults behavior {
  features   [wasm_extension_authoring]
  invariants [extension_operation_atomicity]
  types      [ExtensionDeclaration, ExtensionError]
  ports      [FileSystem]
}

behavior scaffold_wasm_extension_project "Scaffold Wasm Extension Project" {
  category command
  types    [ExtensionDeclaration]
  requires {
    filesystem_available "FileSystem port is available for writing project scaffold files"
  }
  ensures {
    declaration_created                  "An SDK crate declaring the extension (name, version, short name, description) is created in the project directory"
    skeleton_exports_created             "src/ contains a skeleton whose exports the SDK generates (component_guest!)"
    build_target_configured              "The crate builds a wasm32-wasip2 component"
    extension_project_scaffolded_emitted "extension_project_scaffolded event is emitted after successful scaffolding"
  }
  contract """
    When specforge extension init is invoked, the system MUST scaffold a
    new extension crate written with specforge-extension-sdk: a Cargo.toml
    depending on the SDK, a src/lib.rs whose #[extension(name, version,
    short, description)] declares the extension and whose
    component_guest! generates every export, and a cargo configuration
    building a wasm32-wasip2 component. No manifest file is written: the
    binary declares the extension (ADR 0012).
    It MUST refuse to scaffold into a directory that already exists,
    default the extension name when --name is not given, and describe what
    it created as structured JSON under --format=json.
  """
  produces [extension_project_scaffolded]
  verify unit "scaffold creates an SDK crate declaring the extension"
  verify unit "scaffold creates src/ with skeleton exports"
  verify unit "scaffold builds for wasm32-wasip2"
  verify unit "specforge extension init rejects when directory already exists"
  verify unit "specforge extension init --format=json outputs structured JSON"
  verify unit "specforge extension init uses default name when --name not provided"
  verify unit "the scaffold's extension name is a package name"
  verify contract "Scaffold Wasm Extension Project: Wasm extension scaffolding holds — filesystem_available, declaration_created, skeleton_exports_created, build_target_configured, extension_project_scaffolded_emitted"
}

behavior build_wasm_extension "Build Wasm Extension" {
  category command
  requires {
    source_available    "Extension source code exists in the project directory"
    toolchain_available "cargo and the wasm32-wasip2 target are installed"
  }
  ensures {
    wasm_binary_produced    "The component is built at target/wasm32-wasip2/release/<crate>.wasm"
    build_errors_diagnosed  "Build errors are reported as ExtensionError diagnostics"
    extension_built_emitted "extension_built event is emitted after successful build"
  }
  contract """
    When specforge extension build is invoked, the system MUST compile
    the extension crate to a wasm32-wasip2 component (cargo build
    --release --target wasm32-wasip2) and name the component it built.
    Build errors MUST be reported as ExtensionError diagnostics (E040).
    Before compiling, it MUST check that the extension project
    structure exists and fail with a diagnostic when it doesn't.
  """
  produces [extension_built]
  verify unit "build produces .wasm binary"
  verify unit "build errors reported as ExtensionError diagnostics"
  verify unit "specforge extension build validates project structure exists"
  verify contract "Build Wasm Extension: Wasm extension building holds — source_available, toolchain_available, wasm_binary_produced, build_errors_diagnosed, extension_built_emitted"
}

behavior validate_wasm_extension_locally "Validate Wasm Extension Locally" {
  invariants [wasm_sandbox_integrity]
  category   validation
  types      [ExtensionDeclaration, SandboxPolicy, ExtensionError]
  ports      [WasmRuntime, FileSystem]
  requires {
    wasm_binary_available  "A locally built component exists (a .wasm file, or the crate's target/wasm32-wasip2/release component)"
    wasm_runtime_available "WasmRuntime port is available for loading and executing the extension"
    fixtures_available     "Fixture .spec files are shipped with the extension for validation"
  }
  ensures {
    production_sandbox_used              "Extension runs in the same sandbox as production to catch permission errors early"
    export_failures_diagnosed            "Export failures are reported as ExtensionError diagnostics"
    extension_fixtures_validated_emitted "extension_fixtures_validated event is emitted after successful validation"
  }
  contract   """
    When specforge extension validate is invoked, the system MUST load
    the locally built .wasm binary and exercise its declared contribution
    exports (validators, renderers, collectors) against fixture .spec
    files in a sandbox environment. This exercises declared contribution
    exports (validators, renderers, collectors) against fixture .spec
    files shipped with the extension — it is NOT test execution. No user
    test suites are invoked, no test frameworks are loaded, and no test
    results are produced. The output is a pass/fail validation report for
    each contribution export, not a test report. The extension MUST run in the same sandbox as
    production to catch permission errors early. Export failures MUST be
    reported as ExtensionError diagnostics.
    It MUST load the component's declaration as every environment loads it
    (load_extension_declaration) and report what the registry build of
    that declaration alone reports (E030, W021, W138, ...; missing peers
    are not reported, they are installed beside it), with the declaration
    itself under --format json. No built component is an error (E040), and
    a binary that is not an extension is E028.
  """
  produces   [extension_fixtures_validated]
  verify unit "validation loads local .wasm binary"
  verify unit "validation runs against fixtures"
  verify unit "validation uses production sandbox policy"
  verify unit "validation failure reported as ExtensionError"
  verify unit "specforge extension validate reports the declaration's registry build diagnostics"
  verify unit "specforge extension validate errors when no built component is found"
  verify contract "Validate Wasm Extension Locally: local Wasm extension validation holds — wasm_binary_available, wasm_runtime_available, fixtures_available, production_sandbox_used, export_failures_diagnosed, extension_fixtures_validated_emitted"
}

// Implementation detail for publish_to_registry in behaviors/extensions.spec.
// Handles Wasm binary packaging and upload.
behavior publish_wasm_extension "Publish Wasm Extension" {
  invariants [registry_integrity, registry_api_openness]
  category   command
  ports      [FileSystem, RegistryClient]
  requires {
    wasm_binary_available "A built component exists"
    declaration_valid     "The declaration read from the component has been validated"
    registry_available    "RegistryClient port is available for publishing to the configured registry"
  }
  ensures {
    bundle_published            "The .wasm binary and the declaration derived from it are published to the configured registry"
    publish_failures_diagnosed  "Validation or publishing failures are reported as ExtensionError diagnostics"
    extension_published_emitted "extension_published event is emitted after successful publication"
  }
  contract   """
    When specforge publish is invoked, the system MUST publish the .wasm
    binary and the declaration derived from it (its manifest) to the
    configured registry. The declaration MUST be validated before
    publishing. Validation or publishing failures MUST
    be reported as ExtensionError diagnostics.
  """
  produces   [extension_published]
  verify unit "publish uploads the .wasm binary and the declaration derived from it"
  verify unit "the declaration read from the component is validated before publish"
  verify unit "publish failure reported as ExtensionError"
  verify unit "publish refuses a declaration whose name or version is not publishable before it uploads"
  verify contract "Publish Wasm Extension: Wasm extension publishing holds — wasm_binary_available, declaration_valid, registry_available, bundle_published, publish_failures_diagnosed, extension_published_emitted"
}
