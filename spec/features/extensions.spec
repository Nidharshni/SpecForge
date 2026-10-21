// Extension features — CLI extension management and provider-based ref
// validation.
//
// The extension surface is split across three feature files:
//   - features/extensions.spec  — CLI management, providers, registry
//   - features/wasm.spec        — Wasm runtime, host functions, authoring
//   - features/zero-entity-core.spec — manifest schema, registries, LSP, bootstrap

use "behaviors/extensions"
use "behaviors/wasm-lifecycle"
use "invariants/extensions"

// Invariants (offline_first_extension_resolution, registry_api_openness) are
// enforced transitively through behaviors — features inherit invariant coverage
// from their behavior composition lists.

// CLI surface: specforge add is in features/project-init.spec::project_initialization
// (add_extension_to_existing_project behavior). specforge remove and specforge extensions
// are managed here. See also features/wasm.spec for install/uninstall lifecycle.
feature extension_management "Extension Management" {
  status   done
  problem  """
    The core compiler has zero built-in entity types. Teams need to
    install extensions that provide the entity kinds they use, and the
    compiler must gracefully handle references to uninstalled extensions.
  """
  solution """
    Contribution-based extension model: an extension declares entities, edges, shared fields, enhancements, validation rules, surfaces, collectors, analyzers, passes and feature flags (ADR 0012, 0050):
    specforge remove manages
    extension removal from specforge.json (specforge add is part of
    the project initialization feature). specforge extensions lists
    installed extensions. Extension manifests are loaded and their
    entity types registered into the KindRegistry. Cross-extension
    references use soft resolution (I004 if extension missing).
    Wasm runtime loading, peer dependencies, and topological ordering
    are specified in features/wasm.spec.
  """
}

feature provider_based_ref_validation "Provider-Based Ref Validation" {
  status   done
  problem  """
    External references with custom schemes need pluggable validation —
    typos in reference identifiers should be caught at compile time, not
    discovered during a review.
  """
  solution """
    Provider model: providers are Wasm modules that register ref schemes
    and kinds. The compiler delegates validation to the appropriate
    provider via the Wasm runtime. Providers use the specforge.http_get
    host function for network validation. Multiple aliased instances of
    the same provider are supported. Scheme-to-provider routing is managed
    via the SchemeRegistryEntry set.
  """
}

feature extension_registry "Extension Registry" {
  status   done
  // Cross-feature: registry_api_openness invariant also references
  // publish_schema_specification from features/output.spec — the open registry
  // API schema is part of the Graph Protocol publishing surface.
  problem  """
    Extensions are currently resolved from local paths or direct URLs.
    There is no registry API for discovering, searching, or publishing
    extensions. Without a registry, the H2 ecosystem cannot grow —
    extension authors have no standard way to share their work and
    users have no way to discover available extensions.
  """
  solution """
    Registry API with configurable endpoints and an open, published API
    specification (per P6: "the standard is the moat"). The registry API
    schema MUST be published as an open specification so that third-party
    registries can implement it — SpecForge MUST NOT be the only possible
    registry host. specforge.json declares registry configs with scope-based
    routing. specforge search queries every
    configured registry, filtered by declared category. specforge extension publish uploads validated .wasm binaries
    with SHA256 integrity. Downloaded binaries are verified against declared
    hashes and their publisher signatures, and the hash and key id are
    recorded in specforge.lock. First-use MUST NOT require network access —
    local path and git sources work offline; network registries are opt-in
    configuration. The builtins' candidate vocabulary (keyword and field to declaring extension) is generated with them at re-vendor time and handed to the registry build. It powers the suggestions of E024, W151 and W020 for offline use (ADR 0122).
    The bundled keyword-to-extension index is a bootstrap convenience,
    regenerable from any configured registries. It confers no preferential
    status to any particular registry or set of extensions.
  """
}

feature registry_authentication "Registry Authentication" {
  status   done
  problem  """
    Extension registries may require authentication for private or enterprise
    extensions. There is no mechanism to configure credentials, authenticate
    requests, or read a registry that requires a token. Without
    authentication, organizations cannot use private extension repositories.
  """
  solution """
    Registry credential management via specforge login and specforge
    logout. login keeps a token per registry root: a secret in a
    0600 file (--token), or a reference to an environment variable
    (--token-env) or a file (--token-file), never in specforge.json.
    Every request to a registry carries the credential kept for it, so a
    registry that requires a token to read can be used; a rate-limited
    request is retried. Logout removes the stored credential for a given
    registry. Error messages never leak credential details.

    P8 guard: Authentication MUST never be required for first use.
    SpecForge ships no public registry: builtins and local extensions need
    neither a registry nor authentication. Authentication is only required
    by the registries a project configures.
  """
}
