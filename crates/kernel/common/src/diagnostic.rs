use crate::SourceSpan;
use specforge_diagnostics::{Code, GradedCode, Level};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    crate::shape::Shape,
)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// The severity a diagnostic of `code` has: its catalogued level.
    pub const fn of(code: Code) -> Severity {
        match code.level() {
            Level::Error => Severity::Error,
            Level::Warning => Severity::Warning,
            Level::Info => Severity::Info,
            // `Code::catalogued` refuses it: a pass-graded code is a
            // `GradedCode`.
            Level::SetByPass => panic!("a Code never has a level set by its pass"),
        }
    }
}

/// A compiler message. Built from a code ([`Diagnostic::new`],
/// [`Diagnostic::graded`]), or, where a code arrives as text,
/// [`Diagnostic::untyped`]; never as a struct literal outside this crate
/// (`#[non_exhaustive]`), so no site chooses a severity for a core code.
///
/// ```compile_fail,E0639
/// let d = specforge_common::Diagnostic {
///     code: "W112".into(),
///     severity: specforge_common::Severity::Error,
///     message: String::new(),
///     span: None,
///     suggestion: None,
///     data: None,
///     origin: None,
/// };
/// ```
#[non_exhaustive]
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, crate::shape::Shape,
)]
pub struct Diagnostic {
    pub code: String,
    /// The code's catalogued level when built; a diagnostic policy may
    /// raise it afterwards (strict promotion), and nothing else changes it.
    pub severity: Severity,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub suggestion: Option<String>,
    /// What the diagnostic is about, typed, for the consumers that act on
    /// it (the LSP's quick fixes) — so none of them parses `message`, which
    /// is presentation. Absent for most diagnostics, and then serialized
    /// not at all. Boxed so a diagnostic without one stays small (it is
    /// the error type of many `Result`s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Box<DiagnosticData>>,
    /// The extension that reported this diagnostic (a rule's or a pass's
    /// declaring extension); `None` for the host's own. Presentation titles
    /// and explains a code only for its owner (`specforge_diagnostics::describes`),
    /// so a kept finding whose code the extension may not use (W150) is never
    /// described as another owner's. Serialized only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// A diagnostic's structured payload: the values its message names, as
/// data. Serialized with a `kind` tag (`{"kind": "unresolved_reference",
/// …}`); a variant exists only for a diagnostic some consumer acts on.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, crate::shape::Shape,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiagnosticData {
    /// E003: `entity`'s reference field `field` names `target`, which no
    /// entity declares. `did_you_mean` is the closest declared id, when one
    /// is close enough to suggest.
    UnresolvedReference {
        target: String,
        entity: String,
        field: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        did_you_mean: Option<String>,
    },
    /// E025: a `use` import names `path`, which resolves to no `.spec`
    /// file. `did_you_mean` is the closest known path, when one is close.
    UnresolvedImport {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        did_you_mean: Option<String>,
    },
    /// E013, E026: `keyword` — an entity id, or an extension's kind
    /// keyword — collides with a keyword the grammar or an earlier
    /// extension already owns.
    ShadowedKeyword { keyword: String },
    /// W061: the entities of a reference cycle, in path order (the first
    /// one repeated last, closing the cycle).
    ReferenceCycle { path: Vec<String> },
    /// A diagnostic an extension pass raised about `entity` (the pass named
    /// it), whether or not the graph holds that entity.
    Subject { entity: String },
    /// W148: the custom rule `rule`'s `function` failed on these entities
    /// during one check, so they were not checked: every failure, entities
    /// by id (the message names only how many and the first).
    CustomRuleFailures {
        rule: String,
        function: String,
        failures: Vec<CustomRuleFailure>,
    },
}

/// One entity a custom rule's function failed on, with the call's error.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, crate::shape::Shape,
)]
pub struct CustomRuleFailure {
    pub entity: String,
    pub error: String,
}

impl DiagnosticData {
    /// The entities the payload says the diagnostic is about, each once,
    /// in the payload's order: an unresolved reference's holder, a cycle's
    /// entities, a pass diagnostic's subject, the entities a custom rule
    /// could not check. None for the others.
    /// Navigation attributes a diagnostic from these, never its message.
    pub fn entities(&self) -> Vec<&str> {
        let mut entities: Vec<&str> = Vec::new();
        let named: Vec<&str> = match self {
            DiagnosticData::UnresolvedReference { entity, .. }
            | DiagnosticData::Subject { entity } => vec![entity.as_str()],
            DiagnosticData::ReferenceCycle { path } => path.iter().map(String::as_str).collect(),
            DiagnosticData::CustomRuleFailures { failures, .. } => {
                failures.iter().map(|f| f.entity.as_str()).collect()
            }
            DiagnosticData::UnresolvedImport { .. } | DiagnosticData::ShadowedKeyword { .. } => {
                Vec::new()
            }
        };
        for entity in named {
            if !entities.contains(&entity) {
                entities.push(entity);
            }
        }
        entities
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.code, self.message)
    }
}

impl Diagnostic {
    /// A diagnostic of a core code, at the code's catalogued level:
    /// `Diagnostic::new(codes::W112, message)` is a warning.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self::untyped(code.id(), Severity::of(code), message)
    }

    /// An analyze finding (`A###`) at the severity its pass grades it.
    pub fn graded(code: GradedCode, severity: Severity, message: impl Into<String>) -> Self {
        Self::untyped(code.id(), severity, message)
    }

    /// A diagnostic whose code arrives as text: an extension's rule or pass
    /// code, an `OpError` turned back into a diagnostic, or a test's
    /// fixture. Host source names its own codes through `codes::*` and
    /// builds with [`Diagnostic::new`] or [`Diagnostic::graded`] instead.
    pub fn untyped(
        code: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            span: None,
            suggestion: None,
            data: None,
            origin: None,
        }
    }

    /// A diagnostic an extension reported (a rule's or a pass's), as the
    /// extension gave it: its code and severity are kept, and it names
    /// `extension` as its origin, so that presentation never describes the
    /// code as its catalog owner's when the extension may not use it (W150).
    pub fn from_extension(
        extension: impl Into<String>,
        code: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        let mut diagnostic = Self::untyped(code, severity, message);
        diagnostic.origin = Some(extension.into());
        diagnostic
    }

    /// The extension that reported this diagnostic; `None` for the host's own.
    pub fn origin(&self) -> Option<&str> {
        self.origin.as_deref()
    }

    /// Whether this diagnostic is of `code`.
    pub fn is(&self, code: Code) -> bool {
        code.matches(&self.code)
    }

    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_data(mut self, data: DiagnosticData) -> Self {
        self.data = Some(Box::new(data));
        self
    }

    /// Returns true if this diagnostic is an error.
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Extension trait for collections of diagnostics.
pub trait DiagnosticsExt {
    /// Returns true if the collection contains at least one error-severity diagnostic.
    fn has_errors(&self) -> bool;

    /// Returns the number of error-severity diagnostics.
    fn error_count(&self) -> usize;
}

impl DiagnosticsExt for Vec<Diagnostic> {
    fn has_errors(&self) -> bool {
        self.iter().any(|d| d.is_error())
    }

    fn error_count(&self) -> usize {
        self.iter().filter(|d| d.is_error()).count()
    }
}

impl DiagnosticsExt for [Diagnostic] {
    fn has_errors(&self) -> bool {
        self.iter().any(|d| d.is_error())
    }

    fn error_count(&self) -> usize {
        self.iter().filter(|d| d.is_error()).count()
    }
}
