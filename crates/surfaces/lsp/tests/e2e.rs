#[path = "e2e_support/code_actions.rs"]
mod code_actions;
#[path = "e2e_support/completion.rs"]
mod completion;
#[path = "e2e_support/cursor.rs"]
mod cursor;
#[path = "e2e_support/editing.rs"]
mod editing;
#[path = "e2e_support/formatting.rs"]
mod formatting;
#[path = "e2e_support/hover.rs"]
mod hover;
#[path = "e2e_support/integration.rs"]
mod integration;
#[path = "e2e_support/lifecycle.rs"]
mod lifecycle;
#[path = "e2e_support/navigation.rs"]
mod navigation;
#[path = "e2e_support/semantic_tokens.rs"]
mod semantic_tokens;
#[path = "e2e_support/symbols.rs"]
mod symbols;

// What every e2e support file reads through `use super::*`.
#[allow(unused_imports)]
pub use crate::session::{Session, codes, uri_of};
pub use serde_json::{Value, json};
#[allow(unused_imports)]
pub use specforge_test_macros::test as spec;
