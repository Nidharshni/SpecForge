//! A read's value and what the read noticed about the run (ADR 0130): the schema notices of ADR 0111 (I016, W053),
//! then the request's I020. The notices are never part of the value's document; they reach the reply beside it, and
//! only a surface's envelope opens one: MCP's `Answer::noticed` and `ResourceText::noticed`, the CLI's
//! `Notes::noticed`.

use specforge_common::Diagnostic;

/// A read's value with its notices. The value is reached only with the notices: by [`Self::map`] and
/// [`Self::try_map`], which keep them, or by [`Self::into_value_and_notices`], which hands over both and which only
/// the surfaces' envelopes call.
#[must_use = "a read's notices reach the reply: hand it to the surface's envelope"]
#[derive(Debug, Clone, PartialEq)]
pub struct Noticed<T> {
    value: T,
    notices: Vec<Diagnostic>,
}

impl<T> Noticed<T> {
    /// `value`, and what reading it noticed, in the order they are said (ADR 0111 D8).
    pub fn new(value: T, notices: Vec<Diagnostic>) -> Self {
        Self { value, notices }
    }

    /// The same notices over `f(value)`.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Noticed<U> {
        Noticed {
            value: f(self.value),
            notices: self.notices,
        }
    }

    /// The same notices over `f(value)`, or `f`'s refusal, which carries none (ADR 0111 D1).
    pub fn try_map<U, E>(self, f: impl FnOnce(T) -> Result<U, E>) -> Result<Noticed<U>, E> {
        let Self { value, notices } = self;
        f(value).map(|value| Noticed { value, notices })
    }

    /// What the read noticed, to look at; the value stays inside.
    pub fn notices(&self) -> &[Diagnostic] {
        &self.notices
    }

    /// Both, for the envelope that says the notices beside the value. A surface calls it only in its envelopes
    /// (`only_the_envelopes_open_a_noticed_read`, `only_the_notes_open_a_noticed_read`).
    pub fn into_value_and_notices(self) -> (T, Vec<Diagnostic>) {
        (self.value, self.notices)
    }

    /// The same value, with `more` said after the notices it has (I020 after the schema's, ADR 0111 D8).
    pub(crate) fn and_notices(mut self, more: Vec<Diagnostic>) -> Self {
        self.notices.extend(more);
        self
    }

    /// The value, for a read in ops that renders it with its notices (`Noticed<Exploration>::document`).
    #[allow(
        dead_code,
        reason = "the exploration renders its document from it in a later commit"
    )]
    pub(crate) fn value(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::codes;

    fn notice(code: specforge_common::Code, what: &str) -> Diagnostic {
        Diagnostic::new(code, what)
    }

    #[specforge_test_macros::test(
        behavior = "compute_schema_version",
        verify = "every reader of the schema reports the notices export reports for the same record"
    )]
    fn map_and_try_map_keep_the_notices() {
        let i016 = notice(codes::I016, "the schema cache could not be read");
        let mapped = Noticed::new(2, vec![i016.clone()]).map(|n| n * 2);
        assert_eq!(mapped.into_value_and_notices(), (4, vec![i016.clone()]));

        let kept = Noticed::new(2, vec![i016.clone()])
            .try_map(Ok::<_, ()>)
            .unwrap();
        assert_eq!(kept.into_value_and_notices(), (2, vec![i016.clone()]));

        let refused = Noticed::new(2, vec![i016]).try_map(|_| Err::<u8, _>("no"));
        assert_eq!(refused, Err("no"));
    }

    #[test]
    fn more_notices_follow_the_schemas() {
        let i016 = notice(codes::I016, "the schema cache could not be read");
        let i020 = notice(codes::I020, "unknown entity kind 'ghost'");
        let both = Noticed::new((), vec![i016.clone()]).and_notices(vec![i020.clone()]);
        assert_eq!(both.notices(), [i016, i020]);
    }
}
