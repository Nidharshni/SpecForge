use serde::Serialize;
use specforge_parser::VerifyStatement;

/// One `verify` statement as every output writes it. An entity's statements are its own
/// member (`Node::obligations`, ADR 0094); `verify` is reserved syntax (ADR 0002) whose meaning
/// the extensions supply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specforge_common::shape::Shape)]
#[shape(define = "VerifyStatement")]
pub struct Obligation {
    pub kind: String,
    pub description: String,
}

impl Obligation {
    pub fn of(statement: &VerifyStatement) -> Self {
        Self {
            kind: statement.kind.clone(),
            description: statement.description.clone(),
        }
    }
}
