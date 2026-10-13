//! An operation's refusal as the McpError a call answers: the argument its
//! subject is, named by the surface that sent it (ADR 0072, ADR 0033 D2).

use specforge_ops::Refusal;

use crate::mutation::Failed;
use crate::tool::McpError;

impl McpError {
    /// `refusal`'s error, as [`From<OpError>`] makes it, naming the argument
    /// `argument` gives for its subject; none when it has no subject or the
    /// surface fills that field itself.
    pub fn refused<F>(
        refusal: Refusal<F>,
        argument: impl FnOnce(F) -> Option<&'static str>,
    ) -> McpError {
        let named = refusal.subject.and_then(argument);
        let error = McpError::from(refusal.error);
        match named {
            Some(argument) => error.with_argument(argument),
            None => error,
        }
    }
}

impl Failed {
    /// A mutation's refusal: [`McpError::refused`], and what the operation
    /// left written stays reported (ADR 0022 D4).
    pub fn refused<F>(
        mut refusal: Refusal<F>,
        argument: impl FnOnce(F) -> Option<&'static str>,
    ) -> Failed {
        let files = std::mem::take(&mut refusal.error.writes);
        Failed::after(McpError::refused(refusal, argument), files)
    }
}

/// How a tool, prompt or resource names the fields of an operation it calls:
/// each field the operation's refusals can be about, to the argument that
/// sends it, or to `None` for a field the surface fills itself. Implemented on
/// the entry's unit struct, once per field enum, with one `match` and no
/// wildcard arm, so a new field fails to compile until it is named.
pub(crate) trait ArgumentOf<F> {
    fn argument(field: F) -> Option<&'static str>;

    /// `refusal` as the McpError a read answers.
    #[allow(dead_code)] // first used by the read operations' tools
    fn refused(refusal: Refusal<F>) -> McpError {
        McpError::refused(refusal, Self::argument)
    }

    /// `refusal` as a failed mutation.
    fn failed(refusal: Refusal<F>) -> Failed {
        Failed::refused(refusal, Self::argument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_ops::{OpError, OpErrorKind, Writes};

    #[derive(Debug, Clone, Copy)]
    enum Field {
        Named,
        Filled,
    }

    struct Entry;

    impl ArgumentOf<Field> for Entry {
        fn argument(field: Field) -> Option<&'static str> {
            match field {
                Field::Named => Some("x"),
                Field::Filled => None,
            }
        }
    }

    fn error() -> OpError {
        OpError::new(OpErrorKind::InvalidInput, "bad", "bad value")
    }

    #[specforge_test_macros::test(
        behavior = "read_mcp_arguments_as_declared",
        verify = "an operation's refusal of a core tool's call names the argument it is about"
    )]
    fn a_refusal_names_the_argument_its_subject_is() {
        let named = Entry::refused(error().about(Field::Named));
        assert_eq!(named.argument.as_deref(), Some("x"));
        let mut plain = named.clone();
        plain.argument = None;
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::to_value(McpError::from(error())).unwrap()
        );

        let filled = Entry::refused(error().about(Field::Filled));
        assert_eq!(filled.argument, None);
        let subjectless = Entry::refused(Refusal::<Field>::from(error()));
        assert_eq!(subjectless.argument, None);
        // A kind that is not about a value has no subject to name.
        let timeout = OpError::new(OpErrorKind::Timeout, "slow", "slow").about(Field::Named);
        assert_eq!(Entry::refused(timeout).argument, None);
    }

    #[specforge_test_macros::test(
        behavior = "read_mcp_arguments_as_declared",
        verify = "an operation's refusal of a core tool's call names the argument it is about"
    )]
    fn a_failed_mutation_keeps_what_its_operation_left_written() {
        let left = error().with_writes(Writes::of([std::path::PathBuf::from("a.spec")]));
        let failed = Entry::failed(left.about(Field::Named));
        assert_eq!(failed.files.len(), 1);
        assert_eq!(failed.error.argument.as_deref(), Some("x"));
    }
}
