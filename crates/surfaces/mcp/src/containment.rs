//! MCP's spelling of a containment refusal (ADR 0092, ADR 0104).
//!
//! The rule is `specforge_ops::Containment`: it resolves a name the way the file system would and
//! says whether it lands inside a directory. This module builds the bound of a request, and words
//! the refusals MCP makes about a path it targets or opens itself (a call's `path`, init's
//! directory, outline's `file`, render's file).

use std::path::{Path, PathBuf};

use crate::state::McpState;
use crate::tool::{ErrorCode, McpError};
use specforge_ops::extension::{Source, Specified};
use specforge_ops::{Containment, Outside};

/// The bound of a request: the served project's root; while none is served, the server's root;
/// unbounded when the server has neither (a server made with no root names nothing to stay inside).
pub(crate) fn bound(state: &McpState) -> Containment {
    match state.project_root().or(state.root.as_deref()) {
        Some(root) => Containment::within(root),
        None => Containment::unbounded(),
    }
}

/// Where `path` really is: absolute, every symlink followed, a missing tail joined on.
pub(crate) fn landed(path: &Path) -> PathBuf {
    Containment::unbounded()
        .resolve(path)
        .map_or_else(|_| path.to_path_buf(), |held| held.real)
}

/// `invalid_input` on `argument`: what the name resolves to and what bounds it.
pub(crate) fn refusal(outside: &Outside, argument: &str) -> McpError {
    McpError::new(
        ErrorCode::InvalidInput,
        format!(
            "{argument} '{}' resolves to {}, outside {}: a path a request names stays inside the served project",
            outside.named().display(),
            outside.landed().display(),
            outside.root().display()
        ),
    )
    .with_argument(argument)
}

/// `invalid_input` on `file`, a file of the project (not an argument of the call): a source that
/// is a symlink leaving the project.
pub(crate) fn refusal_of_file(outside: &Outside, file: &str) -> McpError {
    McpError::new(
        ErrorCode::InvalidInput,
        format!(
            "{file} resolves to {}, outside {}: a symlink that leaves the project is not followed",
            outside.landed().display(),
            outside.root().display()
        ),
    )
    .with_file(file)
}

/// The local `.wasm` files `specifiers` name, each read against `base` when relative, must resolve
/// inside `bound`: `invalid_input` on `argument` for the first that does not (ADR 0092 D7, closed
/// by ADR 0098). A builtin, a package and a git specifier name no local file.
pub(crate) fn refuse_local_outside(
    bound: &Containment,
    base: &Path,
    specifiers: &[Specified],
    argument: &str,
) -> Result<(), Box<McpError>> {
    for specified in specifiers {
        if let Source::Local(path) = &specified.source {
            let named = base.join(path);
            bound
                .resolve(&named)
                .map_err(|outside| Box::new(refusal(&outside, argument)))?;
        }
    }
    Ok(())
}
