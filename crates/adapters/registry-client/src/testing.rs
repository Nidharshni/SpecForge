//! A package registry in memory, behind the `RegistryClient` seam, and the contract every adapter of the
//! seam keeps (ADR 0044).

mod contract;
mod memory;

pub use contract::{assert_client_contract, assert_private_client_contract};
pub use memory::{Call, CallKind, MemoryClient, package};
