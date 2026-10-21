//! The language server. Its open documents are held in the project session (ADR 0046), each read through one
//! module (ADR 0023); its one reaction to a change applies it, publishes what the project reports and talks to the
//! editor through a port (ADR 0043). Navigation, completion, hover, code actions and formatting answer from the
//! document and the session. It links no package-registry client (ADR 0010).

pub mod answers;
pub mod backend;
mod capabilities;
pub mod changes;
pub mod completion;
mod document;
pub mod editor;
pub mod files;
pub mod hover;
mod navigation;
pub mod publish;
pub mod reaction;
mod state;
mod unwind;
mod uri;
pub mod watchers;

pub use answers::ClientSupport;
pub use capabilities::initialize_result;
pub use document::{
    CompletionSite, Cursor, Document, EntityAt, LineIndex, MOD_DECLARATION, MOD_REFERENCE, Place,
    SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES, Target, Word, WordEdit,
};
pub use hover::hover_field_info;
pub use state::LspState;
