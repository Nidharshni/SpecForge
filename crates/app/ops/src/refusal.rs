//! An operation's refusal and the field of its request it is about (ADR 0072).

use crate::OpError;

/// An operation's refusal: why it failed, and which field of its request the
/// failure is about.
///
/// `F` is the operation's field enum (`extension::AddField`,
/// `export::ExportField`, ...): one variant per field of its request a refusal
/// can be about, named as the request names it. The operation states the
/// subject where it reads the field ([`Refusal::about`]); a surface spells each
/// variant once (MCP: the argument that sends it; the CLI and the LSP present
/// the error and ignore the subject). `subject` is `None` for a refusal of the
/// project or the environment (no project, an unreadable lock, a registry that
/// did not answer, a write the OS refused) and for one raised before any field
/// is read.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal<F> {
    pub error: OpError,
    pub subject: Option<F>,
}

impl<F> Refusal<F> {
    /// `error`, about `field` when its kind is one a value the caller gave
    /// earns ([`crate::OpErrorKind::rejects_a_value`]); about nothing
    /// otherwise, so an operation can state the field once for everything
    /// reading it can refuse (a timeout while resolving a specifier is not
    /// about the specifier).
    pub fn about(field: F, error: OpError) -> Self {
        let subject = error.kind.rejects_a_value().then_some(field);
        Refusal { error, subject }
    }

    /// The same refusal about the field `map` gives for its subject: an
    /// operation that runs another states the other's subject as one of its
    /// own fields, or as none for a field it never sends. Exhaustive in `F`
    /// when `map` is a `match`: a new field of the inner operation fails to
    /// compile there until mapped.
    pub fn map_subject<G>(self, map: impl FnOnce(F) -> Option<G>) -> Refusal<G> {
        Refusal {
            subject: self.subject.and_then(map),
            error: self.error,
        }
    }

    /// Its error, the subject dropped: for a surface that names no field (the
    /// CLI prints `error[CODE]`, the LSP a message).
    pub fn into_error(self) -> OpError {
        self.error
    }
}

/// A refusal about no field: what `?` makes of an [`OpError`] inside an
/// operation (a missing project, an unreadable lock, a failed write).
impl<F> From<OpError> for Refusal<F> {
    fn from(error: OpError) -> Self {
        Refusal {
            error,
            subject: None,
        }
    }
}

impl<F> AsRef<OpError> for Refusal<F> {
    fn as_ref(&self) -> &OpError {
        &self.error
    }
}

impl<F> std::fmt::Display for Refusal<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl<F: std::fmt::Debug> std::error::Error for Refusal<F> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OpErrorKind;
    use specforge_common::codes;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Field {
        One,
        Two,
    }

    const KINDS: [OpErrorKind; 11] = [
        OpErrorKind::InvalidInput,
        OpErrorKind::EntityNotFound,
        OpErrorKind::FileNotFound,
        OpErrorKind::ExtensionNotFound,
        OpErrorKind::Conflict,
        OpErrorKind::SchemaMismatch,
        OpErrorKind::CompilationFailed,
        OpErrorKind::PreconditionFailed,
        OpErrorKind::PermissionDenied,
        OpErrorKind::Timeout,
        OpErrorKind::Internal,
    ];

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn a_refusal_of_a_value_is_about_its_field() {
        for kind in KINDS {
            let refusal = OpError::coded(kind, codes::E027, "x").about(Field::One);
            let expected = matches!(
                kind,
                OpErrorKind::InvalidInput
                    | OpErrorKind::EntityNotFound
                    | OpErrorKind::FileNotFound
                    | OpErrorKind::ExtensionNotFound
                    | OpErrorKind::Conflict
            );
            assert_eq!(kind.rejects_a_value(), expected, "{kind:?}");
            assert_eq!(refusal.subject, expected.then_some(Field::One), "{kind:?}");
            assert_eq!(refusal.error.kind, kind);
        }
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn an_operation_maps_another_operations_subject() {
        let error = OpError::new(OpErrorKind::InvalidInput, "x", "x");
        let to_byte = |f: Field| match f {
            Field::One => Some(7u8),
            Field::Two => None,
        };
        let about = Refusal::about(Field::One, error.clone());
        let mapped = about.clone().map_subject(to_byte);
        assert_eq!(mapped.subject, Some(7));
        assert_eq!(mapped.error, error);
        assert_eq!(about.map_subject(|_| None::<u8>).subject, None);
        let none = Refusal::<Field>::from(error.clone()).map_subject(|_| Some(1u8));
        assert_eq!(none.subject, None);
        let two = Refusal::about(Field::Two, error).map_subject(to_byte);
        assert_eq!(two.subject, None);
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn an_op_error_is_a_refusal_about_nothing() {
        let error = OpError::new(OpErrorKind::InvalidInput, "x", "boom");
        let refusal: Refusal<Field> = error.clone().into();
        assert_eq!(refusal.subject, None);
        assert_eq!(refusal.to_string(), "boom");
        assert_eq!(refusal.as_ref(), &error);
        assert_eq!(refusal.into_error(), error);
    }

    #[specforge_test_macros::test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn a_refusal_stays_small_enough_to_return_whole() {
        // clippy.toml's `large-error-threshold` is 153 bytes.
        assert!(std::mem::size_of::<Refusal<Field>>() <= 153);
    }
}
