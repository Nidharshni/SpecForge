//! Test traceability for SpecForge: a Rust test annotated with `#[specforge_test(...)]` names the spec entity it
//! proves, and this crate records what it proved in the report `specforge collect` reads.

pub mod atexit;
pub mod guard;
pub mod registry;
pub mod report;

/// Re-export the proc macro so users can write `#[specforge_test(...)]`
/// when they `use specforge_test::prelude::*`.
///
/// The attribute registers the test itself; a plain `#[test]` below it
/// is rejected at compile time:
///
/// ```compile_fail
/// use specforge_test::prelude::*;
///
/// #[specforge_test(behavior = "create_user")]
/// #[test]
/// fn creates_a_user() {}
/// ```
///
/// A misspelled argument is an error rather than a silently lost
/// obligation:
///
/// ```compile_fail
/// use specforge_test::prelude::*;
///
/// #[specforge_test(behavior = "create_user", verfy = "rejects a duplicate email")]
/// fn rejects_a_duplicate_email() {}
/// ```
pub mod prelude {
    pub use specforge_test_macros::test as specforge_test;
}

/// Private API consumed by proc macro expansion. Not for direct use.
#[doc(hidden)]
pub mod __private {
    pub use crate::guard::{assert_registered_once, TestGuard};
}
