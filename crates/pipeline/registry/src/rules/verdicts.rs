//! The custom-verdict seam: how a `check: "custom"` rule gets its
//! extension's answer on one entity (ADR 0020 D3). The project's adapter
//! calls the extension (`ExtensionCalls::validate`, ADR 0013); tests answer
//! with a closure; [`NoVerdicts`] stands for "no runtime".

use crate::entity::EntityRecord;

/// A custom rule's verdict on one entity: pass, or fail naming the
/// offending field and value for `{field}` / `{value}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail {
        field: Option<String>,
        value: Option<String>,
    },
}

/// What a verdict is asked about.
#[derive(Debug, Clone, Copy)]
pub enum Subject<'a> {
    /// An entity of the project, as its record.
    Entity(&'a EntityRecord),
    /// The load-time probe: an entity of `kind` (the rule's target kind,
    /// `None` when it names none) that declares nothing.
    Probe { kind: Option<&'a str> },
}

/// One call of a custom rule's function.
#[derive(Debug, Clone, Copy)]
pub struct CustomCall<'a> {
    /// The extension whose module exports `function`.
    pub extension: &'a str,
    pub function: &'a str,
    pub subject: Subject<'a>,
}

/// Why there is no verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerdictError {
    /// No runtime can answer (an environment built without one): the rule
    /// is skipped, nothing is reported.
    Unavailable,
    /// The call failed (a trap, a missing export, an answer that is not a
    /// verdict): W112 at probe time, W148 at check time.
    Failed(String),
}

/// Answers custom rules: one instance serves every rule of a check, since
/// the extension arrives in the [`CustomCall`].
pub trait CustomVerdicts {
    fn verdict(&self, call: CustomCall<'_>) -> Result<Verdict, VerdictError>;
}

impl<F> CustomVerdicts for F
where
    F: Fn(CustomCall<'_>) -> Result<Verdict, VerdictError>,
{
    fn verdict(&self, call: CustomCall<'_>) -> Result<Verdict, VerdictError> {
        self(call)
    }
}

/// No runtime: every verdict is [`VerdictError::Unavailable`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoVerdicts;

impl CustomVerdicts for NoVerdicts {
    fn verdict(&self, _call: CustomCall<'_>) -> Result<Verdict, VerdictError> {
        Err(VerdictError::Unavailable)
    }
}
