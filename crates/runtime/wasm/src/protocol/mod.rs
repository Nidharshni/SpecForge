//! The one loader that reads an extension's declaration
//! ([`load_declaration`], ADR 0012). The wire types are
//! `specforge_protocol_types`, shared with the SDK.

mod load;

pub use load::{Loaded, load_declaration};
