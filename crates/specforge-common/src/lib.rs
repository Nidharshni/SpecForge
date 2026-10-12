// The derive names this crate by its own name, inside it too.
extern crate self as specforge_common;

pub mod cycles;
mod diagnostic;
pub mod discovery;
mod interner;
pub mod package;
pub mod peers;
mod present;
mod project;
mod project_root;
pub mod shape;
mod slug;
mod span;
pub mod structural;
pub mod suggest;

pub use diagnostic::{CustomRuleFailure, Diagnostic, DiagnosticData, DiagnosticsExt, Severity};
pub use discovery::{Walked, discover_spec_files, is_discovered, is_excluded, walk_spec_files};
pub use interner::Sym;
pub use present::{
    Counts, DiagnosticJson, DiagnosticList, MAX_DIAGNOSTICS, compute_exit_code, diagnostic_summary,
    diagnostics_json, format_diagnostic, render_diagnostics, render_plain, serialize_diagnostics,
    truncate_diagnostics,
};
pub use project::{
    ConfigProblem, ConfigRead, ExtensionEntry, InferenceConfig, ProjectConfig,
    extension_entry_name, load_project_config, parse_project_config, read_project_config,
    validate_project_name,
};
pub use project_root::{NoSuchPath, PROJECT_FILE, ProjectRoot};
pub use slug::slug;
pub use span::SourceSpan;
/// Core diagnostic codes as typed constants (`codes::W112`), so a host
/// crate builds a diagnostic with `Diagnostic::new(codes::W112, …)`.
pub use specforge_diagnostics::{Code, GradedCode, codes};
pub use suggest::find_close_match;
