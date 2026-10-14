//! A package registry in memory beside the `Registry` port, and the contract every adapter of the port
//! keeps (ADR 0044), as `specforge_installed::testing` and `specforge_wasm::testing` sit beside theirs.

mod contract;
mod memory;
#[cfg(test)]
mod tests;

pub use contract::{assert_registry_contract, contract_packages};
pub use memory::{Asked, IN_MEMORY_KEY, MemoryRegistry, Published, declaration};
