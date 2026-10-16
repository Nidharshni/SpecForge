//! What the caller of an operation lends it (ADR 0124): what the call may reach on disk and who can be asked.
//! A surface builds one per command or call and hands it, unchanged, to every request it fills; an operation
//! reads its parts where it opens a file (ADR 0104) or puts a question (ADR 0110).

use crate::Containment;
use crate::asking::Asking;

/// What a caller lends one call. Required in every request that opens a project file or can ask
/// (no `Default`: a caller that forgets it does not compile, ADR 0104 D3).
#[derive(Debug, Clone, Copy)]
pub struct Caller<'a> {
    containment: &'a Containment,
    asking: Asking<'a>,
}

impl<'a> Caller<'a> {
    /// A caller that reaches `containment` and puts its questions to `asking`.
    #[must_use]
    pub const fn new(containment: &'a Containment, asking: Asking<'a>) -> Self {
        Caller {
            containment,
            asking,
        }
    }

    /// What the call may reach: resolve every project file through it (ADR 0104 D2).
    #[must_use]
    pub const fn containment(&self) -> &'a Containment {
        self.containment
    }

    /// Who can be asked (ADR 0110): hand it to the registry's `TrustPolicy`, or to collect's consent.
    #[must_use]
    pub const fn asking(&self) -> Asking<'a> {
        self.asking
    }
}

#[cfg(all(test, feature = "testing"))]
#[allow(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn a_caller_hands_back_what_it_was_lent() {
        let dir = tempfile::tempdir().unwrap();
        let bound = Containment::within(dir.path());
        let caller = Caller::new(&bound, Asking::Yes);
        let copy = caller;
        assert_eq!(copy.containment().root(), bound.root());
        assert_eq!(caller.containment(), &bound);
        assert_eq!(format!("{:?}", caller.asking()), "Yes");
    }

    #[test]
    fn the_test_callers_ask_nobody_and_reach_everything_or_their_bound() {
        let dir = tempfile::tempdir().unwrap();
        let bound = Containment::within(dir.path());
        assert_eq!(crate::testing::caller().containment().root(), None);
        assert_eq!(crate::testing::bounded(&bound).containment(), &bound);
        assert_eq!(format!("{:?}", crate::testing::caller().asking()), "Nobody");
        assert_eq!(
            format!("{:?}", crate::testing::bounded(&bound).asking()),
            "Nobody"
        );
    }
}
