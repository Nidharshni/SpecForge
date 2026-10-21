//! A spec file a request names (a typed create's `file`, outline's `file`, the fixes' `file_path`): read into its
//! source key, the path relative to the spec root that the compile and the editor name it by, or refused once
//! (ADR 0195). Every spelling of one file is that file; the key is what the ledger, the LSP and a span's `file` say.

use std::path::{Component, Path, PathBuf};

use super::ProjectView;
use crate::{OpError, OpErrorKind};

/// A create's file is not a project source.
pub const NOT_A_SOURCE: &str = "not_a_source";

const FILE_NOT_FOUND: &str = "file_not_found";

/// `named`'s source key under `spec_root`, with `.` and doubled separators gone; `None` when it leaves the spec root
/// (a `..`, or an absolute path elsewhere).
fn key_of(spec_root: &Path, named: &str) -> Option<String> {
    let key = specforge_project::source_key(spec_root, &spec_root.join(named));
    let mut rebuilt = PathBuf::new();
    for component in Path::new(&key).components() {
        match component {
            Component::Normal(name) => rebuilt.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(rebuilt.to_string_lossy().into_owned())
}

fn outside(named: &str) -> OpError {
    OpError::new(
        OpErrorKind::InvalidInput,
        NOT_A_SOURCE,
        format!("'{named}' is outside the spec root: a spec file is named from the spec root"),
    )
}

impl ProjectView<'_> {
    /// The key of the project-root reading of `named` when that is a compiled source and the spec-root reading is
    /// not: the mistake of naming a file as `format`'s paths do.
    fn project_root_reading(&self, named: &str) -> Option<String> {
        let root = self.root()?;
        let spec_root = self.env().spec_root();
        (root != spec_root).then_some(())?;
        let key = key_of(spec_root, &root.join(named).to_string_lossy())?;
        self.project()
            .source_keys()
            .contains(key.as_str())
            .then_some(key)
    }

    fn did_you_mean(&self, named: &str) -> Option<String> {
        self.project_root_reading(named)
            .map(|key| format!("did you mean '{key}'? a spec file is named from the spec root"))
    }

    /// The source key of the compiled source `named` names. A spelling that leaves the spec root is `not_a_source`
    /// (kind `InvalidInput`); any other that is none of the compile's sources (missing, a directory, not a `.spec`
    /// file, excluded) is `file_not_found`. Reads no file.
    pub fn source(&self, named: &str) -> Result<String, OpError> {
        let key = key_of(self.env().spec_root(), named).ok_or_else(|| outside(named))?;
        if self.project().source_keys().contains(key.as_str()) {
            return Ok(key);
        }
        let error = OpError::new(
            OpErrorKind::FileNotFound,
            FILE_NOT_FOUND,
            format!("'{named}' is none of this project's sources"),
        );
        Err(match self.did_you_mean(named) {
            Some(suggestion) => error.with_suggestion(suggestion),
            None => error,
        })
    }

    /// The key of a file a typed edit may create or change: a compiled source, or a new file discovery would keep.
    /// `not_a_source` (kind `InvalidInput`) otherwise.
    pub fn source_place(&self, named: &str) -> Result<String, OpError> {
        let key = key_of(self.env().spec_root(), named).ok_or_else(|| outside(named))?;
        if self.project().source_keys().contains(key.as_str()) {
            return Ok(key);
        }
        let suggestion = self.did_you_mean(named);
        if suggestion.is_none() && self.env().holds_source(&key) {
            return Ok(key);
        }
        let error = OpError::new(
            OpErrorKind::InvalidInput,
            NOT_A_SOURCE,
            format!(
                "'{named}' is not a source of this project (a .spec file inside the spec root, not excluded)"
            ),
        );
        Err(match suggestion {
            Some(suggestion) => error.with_suggestion(suggestion),
            None => error,
        })
    }
}
