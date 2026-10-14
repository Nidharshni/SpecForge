//! The recorded test report as operations fail with it: a report that
//! cannot be used is one [`OpError`], classified here and nowhere else
//! (ADR 0029 D1).

use specforge_common::codes;
use specforge_project::coverage::{self, ReportError, TestReport};
use std::path::Path;
use std::sync::Arc;

use crate::{OpError, OpErrorKind};

/// What to do about a report that is not a `specforge-report.json`, or
/// cannot be read for a reason the OS gave no more specific advice for.
const REWRITE: &str =
    "run `specforge collect` again to rewrite the report, or fix or remove the file";

/// A test report that cannot be used, as every operation fails with it:
/// code E045, of kind
///
/// - `SchemaMismatch` when it is not a `specforge-report.json`;
/// - the OS error's kind when it cannot be read
///   ([`OpErrorKind::of_io_kind`]: `PermissionDenied`, `FileNotFound` for a
///   named file that does not exist, else `Internal`).
///
/// The message is the error's text; the suggestion says what to do about
/// that kind.
pub(crate) fn unusable(error: ReportError) -> OpError {
    let (kind, suggestion) = match &error {
        ReportError::Malformed { .. } => (OpErrorKind::SchemaMismatch, REWRITE),
        ReportError::Unreadable { io, .. } => {
            let kind = OpErrorKind::of_io_kind(*io);
            let suggestion = match kind {
                OpErrorKind::FileNotFound => "check the path: the named test report does not exist",
                OpErrorKind::PermissionDenied => "check the file's permissions",
                _ => REWRITE,
            };
            (kind, suggestion)
        }
    };
    OpError::coded(kind, codes::E045, error.to_string()).with_suggestion(suggestion)
}

/// The report at `path` (`--test-results`, MCP `test_results`), which must
/// exist.
pub(crate) fn named(path: &Path) -> Result<Arc<TestReport>, OpError> {
    coverage::read_report_file(path)
        .map(Arc::new)
        .map_err(unusable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::path::PathBuf;

    fn unreadable(io: ErrorKind) -> ReportError {
        ReportError::Unreadable {
            path: PathBuf::from("r.json"),
            io,
            detail: "the OS said no".into(),
        }
    }

    #[test]
    fn classifies_each_report_failure() {
        let malformed = unusable(ReportError::Malformed {
            path: PathBuf::from("r.json"),
            detail: "EOF".into(),
        });
        assert_eq!(malformed.kind, OpErrorKind::SchemaMismatch);
        assert_eq!(malformed.suggestion.as_deref(), Some(REWRITE));

        for (io, kind) in [
            (ErrorKind::PermissionDenied, OpErrorKind::PermissionDenied),
            (ErrorKind::NotFound, OpErrorKind::FileNotFound),
            (ErrorKind::IsADirectory, OpErrorKind::Internal),
        ] {
            let error = unusable(unreadable(io));
            assert_eq!(error.kind, kind, "{io:?}");
            assert_eq!(error.code, "E045");
            assert_eq!(
                error.message,
                "cannot read test results r.json: the OS said no"
            );
        }
        let missing = unusable(unreadable(ErrorKind::NotFound));
        assert!(missing.suggestion.unwrap().contains("check the path"));
    }
}
