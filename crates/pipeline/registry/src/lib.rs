//! The Registry build: extension declarations in load order turned into kind, field and edge registries (each edge
//! type resolved once, ADR 0060), the rule set, the pass order and the graph inputs, and the checks it runs over a
//! built graph's entity records in one order (ADR 0031). Callers reach it only through `build_registries` and the
//! build's methods. Not the package registry, which is `specforge-registry-client` and `-server`.

// Module groups
mod checks;
pub mod compilation;
pub mod entity;
mod registries;
pub mod rules;

#[cfg(test)]
mod invariants;

// --- Core registries ---
pub use registries::{
    EdgeRegistry, EdgeRegistryEntry, EdgeWriter, FieldRegistry, FieldRegistryEntry, KindRegistry,
    KindRegistryEntry, UnknownFieldType,
};

// --- Extension vocabulary (shared with the SDK through the protocol types) ---
pub use specforge_protocol_types::{CheckKind, ConstraintKind, FieldType, ProofRole};
// --- What a registry entry embeds: the descriptor its extension declared ---
pub use specforge_protocol_types::{EdgeTypeDescriptor, EntityKindDescriptor, FieldDescriptor};

// --- What an extension declares, as the registry build reads it (ADR 0012) ---
pub use specforge_protocol_types::PeerDependency;

// --- The surfaces registry ---
pub use surface::{
    CommandArgType, SurfaceRegistryEntry, SurfaceType, refuse_malformed_tool_schemas,
    register_surface_contributions,
};

// --- What the checks after the graph build read about an entity (ADR 0019) ---
pub use entity::{
    Direction, EdgeCounts, EdgeRecord, EntityRecord, Exemption, FieldRecord, MethodRecord,
    ObligationRecord, ParamRecord, RuleInput,
};

// --- Registry compilation (plan 05): one build, and the graph checks ---
pub use compilation::{BuildPart, CHECK_PHASE, DeclaredPass, RegistryBuild, build_registries};

// Module paths external code names directly (`specforge_registry::surface::`).
pub mod surface;
