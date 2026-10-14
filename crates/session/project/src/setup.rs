//! What part of a project's setup a diagnostic of its [`Environment`] is
//! about (CONTEXT "Setup part").
//!
//! The part is decided where the diagnostic is raised: the registry build
//! tags its own ([`BuildPart`]), the extension load keeps an entry's on the
//! entry, providers tag theirs. A reader of the environment (doctor today)
//! gets it from [`Environment::setup_diagnostics`] and reads no diagnostic
//! code to find it out.

use specforge_common::Diagnostic;
use specforge_registry::BuildPart;

#[cfg(doc)]
use crate::Environment;

/// What part of a project's setup an environment diagnostic is about.
/// Every diagnostic `specforge check` reports before any source has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetupPart {
    /// `specforge.json`, not used as written (E069).
    Config,
    /// `specforge.lock`, which cannot be read (E033), once before the first
    /// entry it left unloaded.
    Lock,
    /// The `extensions` entry at this index of [`Environment::enabled`]: why
    /// it did not load (E028, E070, E072), or what it reported though it
    /// loaded (W149, W153, W138).
    Entry(usize),
    /// No extension loaded, so the compile checks structure only (I002).
    StructuralOnly,
    /// A configured provider that cannot be registered (W118).
    Providers,
    /// One extension declaration on its own (E030, W021, W145).
    Declaration,
    /// A peer requirement (E073, E027; ADR 0041).
    Peers,
    /// What the registries hold (W019, W021, I004, W017).
    Registries,
    /// A declared rule (W150, W147, W112).
    Rules,
    /// An extension's surfaces (E055).
    Surfaces,
    /// Two contributions claim one name: a kind keyword (E026), an edge label
    /// (W018), a rule code (W023), a command, MCP tool or resource name
    /// (E039), a provider scheme (E057).
    Collision,
}

impl From<BuildPart> for SetupPart {
    fn from(part: BuildPart) -> Self {
        match part {
            BuildPart::Declaration => SetupPart::Declaration,
            BuildPart::Peers => SetupPart::Peers,
            BuildPart::Registries => SetupPart::Registries,
            BuildPart::Rules => SetupPart::Rules,
            BuildPart::Surfaces => SetupPart::Surfaces,
            BuildPart::Collision => SetupPart::Collision,
        }
    }
}

/// One diagnostic of an environment and the part of the setup it is about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetupDiagnostic<'a> {
    pub part: SetupPart,
    pub diagnostic: &'a Diagnostic,
}
