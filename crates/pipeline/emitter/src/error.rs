use specforge_diagnostics::{Code, codes};

/// Why an export could not be produced. Each failure is a variant carrying
/// what its message names; [`EmitterError::code`] is the catalogued
/// diagnostic it is. Its `Display` is the message alone, never its code:
/// a surface prints the code from [`EmitterError::code`]. What kind of
/// failure it is to an operation is decided by `specforge_ops` (the
/// emitter does not know operations, ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitterError {
    /// The scope names no entity of the graph (E003).
    ScopeNotFound { entity_id: String },
    /// The token budget cannot hold the export (E062): `reason` says what
    /// does not fit.
    BudgetTooSmall { reason: String },
    /// The export could not be serialized: an emitter bug, no code.
    Serialization(String),
}

impl EmitterError {
    /// The catalogued diagnostic this failure is: E003, E062; none for a
    /// serialization failure.
    pub fn code(&self) -> Option<Code> {
        match self {
            Self::ScopeNotFound { .. } => Some(codes::E003),
            Self::BudgetTooSmall { .. } => Some(codes::E062),
            Self::Serialization(_) => None,
        }
    }
}

impl std::fmt::Display for EmitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScopeNotFound { entity_id } => {
                write!(f, "unresolved entity '{entity_id}' — not found in graph")
            }
            Self::BudgetTooSmall { reason } => f.write_str(reason),
            Self::Serialization(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for EmitterError {}
