// Migration features — spec file format version migration
//
// P7 Justification — Why migration lives in core, not an extension:
// Migration is infrastructure that runs before extensions load. It transforms
// .spec file syntax (indentation, field ordering, block layout) structurally —
// no entity kinds, field semantics, or domain vocabulary are inspected. This
// parallels the formatting precedent (features/formatting.spec): core owns
// generic CST-level operations; extensions contribute domain-specific migration
// via Wasm hooks (ExtensionDeclaration.migration_hook). The P7 test — "does this require
// a compiler change when a new domain appears?" — is satisfied: adding a new
// extension requires zero changes to the migration engine.
// See also: migration_as_core_infrastructure ADR in governance/decisions.spec.

use "behaviors/migration"
use "events/compilation"
use "invariants/migration"
use "types/migration"

feature spec_file_migration "Spec File Migration" {
  status   done
  // MCP: provide_mcp_migrate_tool in features/mcp.spec::mcp_mutation_tools
  problem  """
    As the .spec file format evolves across versions, existing projects
    accumulate files in older formats. There is no automated way to detect
    version mismatches, preview changes, or migrate files to the current
    format while ensuring structural integrity is preserved.
  """
  solution """
    specforge migrate detects format version mismatches, transforms .spec
    files to the current version with automatic backups, supports --dry-run
    for previewing changes as POSIX unified diffs, and validates post-migration
    integrity by comparing pre and post graph structures. A pre-migration
    snapshot is captured before any transforms to enable structural comparison.
    A file that cannot be migrated leaves every file as it was, and the report
    names each such file; files
    already at the target version are skipped (idempotency). Extension schema
    migration (when extension manifests evolve) is delegated to extension
    authors via Wasm migration hooks declared in ExtensionDeclaration.migration_hook.
    Post-migration validation runs once after both core and extension hooks
    complete. Core migration covers only .spec file format syntax evolution.
    See also: graph_protocol_versioning in features/output.spec for the Graph
    Protocol versioning link.
  """
}
