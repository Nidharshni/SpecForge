//! `specforge format`, MCP `specforge.format` and the LSP's formatting
//! requests: one document, or every source of a project, formatted with the
//! configuration `specforge format` uses for each file (ADR 0021).
//!
//! A file that can't be read or written is recorded and the run goes on, so
//! a failure on one file never leaves the others half-done. The surfaces
//! present what happened: the CLI prints it, MCP returns it, the LSP turns
//! its edits into positions.

mod document;
mod edits;
mod run;
mod targets;
#[cfg(test)]
mod tests;

pub use document::{
    ConfigSource, Edit, EditorOptions, FormattedDocument, Lines, Place, config, document, forms,
};
pub use run::{
    Failure, FileChange, FileDiff, FileFailure, FormatDocument, FormatField, Mode, Outcome,
    Request, UNREADABLE, UNWRITABLE, run,
};
