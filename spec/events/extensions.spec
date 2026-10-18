// Extension registration and registry events

use "types/wasm"
use "types/zero-entity-core"

// ── Extension Registration Events ──────────────────────────────

event extension_entity_types_registered "Extension Entity Types Registered" {
  channel "extensions.entity_types_registered"
  payload ExtensionEntityTypesRegisteredPayload
  // observability-only — emitted for tracing/logging, no downstream consumers
  verify integration "emits after extension entity types are registered"
}

type ExtensionEntityTypesRegisteredPayload "Extension Entity Types Registered Payload" {
  extensionName   string
  entityKindCount integer
  edgeTypeCount   integer
  verify unit "ExtensionEntityTypesRegisteredPayload schema is valid"
}

event provider_schemes_registered "Provider Schemes Registered" {
  channel "extensions.provider_schemes_registered"
  payload ProviderSchemesRegisteredPayload
  // observability-only — emitted for tracing/logging, no downstream consumers
  verify integration "emits after provider schemes are registered"
}

type ProviderSchemesRegisteredPayload "Provider Schemes Registered Payload" {
  extensionName string
  schemeCount   integer
  verify unit "ProviderSchemesRegisteredPayload schema is valid"
}

event registry_integrity_verified "Registry Integrity Verified" {
  channel "extensions.registry_integrity_verified"
  payload RegistryIntegrityVerifiedPayload
  // observability-only — emitted for tracing/logging, no downstream consumers
  verify integration "emits after registry integrity check completes"
}

type RegistryIntegrityVerifiedPayload "Registry Integrity Verified Payload" {
  extensionName string
  hashMatch     boolean
  verify unit "RegistryIntegrityVerifiedPayload schema is valid"
}

// ── Provider Events ──────────────────────────────────────

event provider_configured "Provider Configured" {
  channel "extensions.provider_configured"
  payload ProviderConfiguredPayload
  verify integration "emits provider_configured with correct alias and schemeCount"
}

type ProviderConfiguredPayload "Provider Configured Payload" {
  alias         string
  extensionName string
  schemeCount   integer
  kindCount     integer
  verify unit "ProviderConfiguredPayload schema is valid"
}

event provider_ref_validated "Provider Ref Validated" {
  channel "extensions.provider_ref_validated"
  payload ProviderRefValidatedPayload
  verify integration "emits provider_ref_validated with correct scheme and validation result"
}

type ProviderRefValidatedPayload "Provider Ref Validated Payload" {
  scheme        string
  kind          string
  identifier    string
  valid         boolean
  providerAlias string
  verify unit "ProviderRefValidatedPayload schema is valid"
}

// ── Registry Events ─────────────────────────────────────────

event registry_resolved "Registry Resolved" {
  channel "extensions.registry_resolved"
  payload RegistryResolvedPayload
  verify integration "emits registry_resolved with correct extensionName and version"
  verify integration "consumer verify_registry_integrity receives event"
}

type RegistryResolvedPayload "Registry Resolved Payload" {
  extensionName string
  registryAlias string
  version       string
  wasmSizeBytes integer
  verify unit "RegistryResolvedPayload schema is valid"
}

event registry_search_completed "Registry Search Completed" {
  channel "extensions.registry_search_completed"
  payload RegistrySearchCompletedPayload
  verify integration "emits registry_search_completed with correct totalResults"
}

type RegistrySearchCompletedPayload "Registry Search Completed Payload" {
  query             string
  totalResults      integer
  registriesQueried integer
  verify unit "RegistrySearchCompletedPayload schema is valid"
}

event extension_published_to_registry "Extension Published to Registry" {
  channel "extensions.extension_published_to_registry"
  payload ExtensionPublishedToRegistryPayload
  verify integration "emits extension_published_to_registry with correct extensionName and sha256"
}

type ExtensionPublishedToRegistryPayload "Extension Published To Registry Payload" {
  extensionName string
  version       string
  registryAlias string
  sha256        string
  publishTimeMs integer
  verify unit "ExtensionPublishedToRegistryPayload schema is valid"
}

event registries_configured "Registries Configured" {
  channel "extensions.registries_configured"
  payload RegistriesConfiguredPayload
  verify integration "emits registries_configured after registry configuration is parsed"
}

type RegistriesConfiguredPayload "Registries Configured Payload" {
  registryCount integer
  configs       RegistryConfig[]
  verify unit "RegistriesConfiguredPayload schema is valid"
}

// ── Registry Authentication Events ──────────────────────────

event registry_authenticated "Registry Authenticated" {
  channel "extensions.registry_authenticated"
  payload RegistryAuthenticatedPayload
  verify integration "emits registry_authenticated with correct registryUrl and authMethod"
}

type RegistryAuthenticatedPayload "Registry Authenticated Payload" {
  registryUrl string
  authMethod  string
  success     boolean
  timestamp   timestamp
  verify unit "RegistryAuthenticatedPayload schema is valid"
}

event registry_credentials_validated "Registry Credentials Validated" {
  channel "extensions.registry_credentials_validated"
  payload RegistryCredentialsValidatedPayload
  verify integration "emits registry_credentials_validated with correct registryUrl and valid status"
}

type RegistryCredentialsValidatedPayload "Registry Credentials Validated Payload" {
  registryUrl string
  valid       boolean
  timestamp   timestamp
  verify unit "RegistryCredentialsValidatedPayload schema is valid"
}

event registry_logged_out "Registry Logged Out" {
  channel "extensions.registry_logged_out"
  payload RegistryLoggedOutPayload
  verify integration "emits registry_logged_out with correct registryAlias"
}

type RegistryLoggedOutPayload "Registry Logged Out Payload" {
  registryAlias string
  timestamp     timestamp
  verify unit "RegistryLoggedOutPayload schema is valid"
}

// support_private_registries has no dedicated event — it is a composite
// behavior that delegates to authenticate_registry_request (which emits
// registry_authenticated) and resolve_registry_source (which emits
// registry_resolved). No additional event is needed.

event registry_request_retry_exhausted "Registry Request Retry Exhausted" {
  channel "extensions.registry_request_retry_exhausted"
  payload RegistryRequestRetryExhaustedPayload
  verify integration "emits after all retry attempts are exhausted"
}

type RegistryRequestRetryExhaustedPayload "Registry Request Retry Exhausted Payload" {
  registryUrl    string
  lastStatusCode integer
  retryCount     integer
  timestamp      timestamp
  verify unit "RegistryRequestRetryExhaustedPayload schema is valid"
}

event extension_removed "Extension Removed" {
  channel "extensions.extension_removed"
  payload ExtensionRemovedPayload
  verify integration "removing extension emits event with unregistered kinds"
}

type ExtensionRemovedPayload "Extension Removed Payload" {
  extension_name            string
  entity_kinds_unregistered string[]
  verify unit "ExtensionRemovedPayload schema is valid"
}
