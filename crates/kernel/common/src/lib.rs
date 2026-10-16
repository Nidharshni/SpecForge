//! What every host crate shares: the diagnostic and its source span, the interned symbol, the project root and its
//! `specforge.json`, `.spec` discovery, the JSON shape a type serializes to (ADR 0048) and the forms every surface
//! prints diagnostics in. Its SpecForge dependencies are the leaf crates and the shape derive; each carries its
//! reason in `Cargo.toml`.

// The derive names this crate by its own name, inside it too.
extern crate self as specforge_common;

pub mod cycles;
mod diagnostic;
pub mod discovery;
mod exclude;
mod interner;
pub mod package;
pub mod peers;
mod present;
mod project;
mod project_path;
mod project_root;
pub mod shape;
mod slug;
mod span;
pub mod structural;
pub mod suggest;

pub use diagnostic::{CustomRuleFailure, Diagnostic, DiagnosticData, DiagnosticsExt, Severity};
pub use discovery::{SourceRoot, Walked, walk_files, walk_spec_files};
pub use exclude::{Exclude, ExcludeEntry, ExcludeError};
pub use interner::Sym;
pub use present::{
    Counts, DiagnosticDocument, DiagnosticJson, DiagnosticList, MAX_DIAGNOSTICS, compute_exit_code,
    diagnostic_summary, diagnostics_json, format_diagnostic, render_diagnostics, render_plain,
    serialize_diagnostics, truncate_diagnostics,
};
pub use project::{
    ConfigProblem, ConfigRead, ExtensionEntry, InferenceConfig, ProjectConfig,
    extension_entry_name, parse_project_config, read_project_config, validate_project_name,
};
pub use project_path::ProjectPath;
pub use project_root::{NoSuchPath, PROJECT_FILE, ProjectRoot};
pub use slug::slug;
pub use span::SourceSpan;
/// Core diagnostic codes as typed constants (`codes::W112`), so a host
/// crate builds a diagnostic with `Diagnostic::new(codes::W112, …)`.
pub use specforge_diagnostics::{Code, GradedCode, codes};
pub use suggest::find_close_match;
