//! The codes a registry's error answers carry (`{"error": {"code": …}}`).

use serde::{Deserialize, Serialize};

/// The code a registry's error answer carries (`{"error": {"code": …}}`), named in SCREAMING_SNAKE_CASE on the wire.
/// Server and client name it the same; a client reads a code it does not know, or none, as [`ErrorCode::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Nothing is published under the name or version asked for.
    NotFound,
    /// The bearer token is missing, unknown, expired or revoked.
    Unauthorized,
    /// The token is valid but may not do this (a scope it does not cover).
    Forbidden,
    /// The endpoint needs an admin token.
    AdminRequired,
    /// The scope is owned by another account.
    ScopeOwned,
    /// Too many publishes inside the window; the answer carries `Retry-After`.
    RateLimited,
    /// The request is malformed (a missing form field).
    BadRequest,
    /// The upload is larger than the registry accepts.
    PayloadTooLarge,
    /// The upload names no scoped package.
    InvalidName,
    /// The upload names no SemVer version.
    InvalidVersion,
    /// A publish without a signature.
    UnsignedPackage,
    /// The upload's signature is not a package signature, or does not verify over the upload (ADR 0061).
    InvalidSignature,
    /// The uploaded binary is not a Wasm module.
    InvalidWasm,
    /// The uploaded manifest is not an extension declaration.
    InvalidManifest,
    /// The manifest names another package or version than the upload path.
    NameMismatch,
    /// The version is already published.
    DuplicateVersion,
    /// The stored binary does not hash to its recorded digest.
    IntegrityViolation,
    /// A database query failed.
    DbError,
    /// A database task did not finish.
    DbTask,
    /// A token check did not finish.
    AuthTask,
    /// An integrity check did not finish.
    IntegrityTask,
    /// A storage write or commit failed.
    StorageError,
    /// A storage read did not finish.
    StorageTask,
    /// A code this client does not know, or an answer without one.
    #[default]
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// Its wire name (`NOT_FOUND`); `Display` writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::AdminRequired => "ADMIN_REQUIRED",
            Self::ScopeOwned => "SCOPE_OWNED",
            Self::RateLimited => "RATE_LIMITED",
            Self::BadRequest => "BAD_REQUEST",
            Self::PayloadTooLarge => "PAYLOAD_TOO_LARGE",
            Self::InvalidName => "INVALID_NAME",
            Self::InvalidVersion => "INVALID_VERSION",
            Self::UnsignedPackage => "UNSIGNED_PACKAGE",
            Self::InvalidSignature => "INVALID_SIGNATURE",
            Self::InvalidWasm => "INVALID_WASM",
            Self::InvalidManifest => "INVALID_MANIFEST",
            Self::NameMismatch => "NAME_MISMATCH",
            Self::DuplicateVersion => "DUPLICATE_VERSION",
            Self::IntegrityViolation => "INTEGRITY_VIOLATION",
            Self::DbError => "DB_ERROR",
            Self::DbTask => "DB_TASK",
            Self::AuthTask => "AUTH_TASK",
            Self::IntegrityTask => "INTEGRITY_TASK",
            Self::StorageError => "STORAGE_ERROR",
            Self::StorageTask => "STORAGE_TASK",
            Self::Unknown => "UNKNOWN",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_code_is_its_wire_name() {
        use ErrorCode::*;
        for code in [
            NotFound,
            Unauthorized,
            Forbidden,
            AdminRequired,
            ScopeOwned,
            RateLimited,
            BadRequest,
            PayloadTooLarge,
            InvalidName,
            InvalidVersion,
            UnsignedPackage,
            InvalidSignature,
            InvalidWasm,
            InvalidManifest,
            NameMismatch,
            DuplicateVersion,
            IntegrityViolation,
            DbError,
            DbTask,
            AuthTask,
            IntegrityTask,
            StorageError,
            StorageTask,
            Unknown,
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), code.as_str());
            assert_eq!(code.to_string(), code.as_str());
            assert_eq!(
                serde_json::from_value::<ErrorCode>(code.as_str().into()).unwrap(),
                code
            );
        }
    }
}
