//! What a surface reports from a project's diagnostics: lint profiles and
//! strict promotion, in one place for `check`, `analyze` and MCP
//! `validate` and `analyze`.

use std::fmt;
use std::str::FromStr;

use specforge_common::{Diagnostic, Severity};

/// The names of the lint profiles, as `specforge check --lint` and MCP
/// validate's `lint` take them. Any other name is refused.
pub const LINT_PROFILE_NAMES: &[&str] = &["inferred", "pedantic"];

/// A lint profile: a closed set of extra checks a surface may ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintProfile {
    /// The inference-quality checks I200 and I202 over
    /// `specforge-infer.json`, when it exists.
    Inferred,
    /// The explicit name of the default: info diagnostics are always
    /// reported, so it adds nothing.
    Pedantic,
}

impl LintProfile {
    /// Every profile, in the order their diagnostics are added.
    pub const ALL: [LintProfile; 2] = [LintProfile::Inferred, LintProfile::Pedantic];

    /// The profile's name, one of [`LINT_PROFILE_NAMES`].
    pub fn name(self) -> &'static str {
        match self {
            LintProfile::Inferred => "inferred",
            LintProfile::Pedantic => "pedantic",
        }
    }
}

impl fmt::Display for LintProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for LintProfile {
    type Err = UnknownLintProfile;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "inferred" => Ok(LintProfile::Inferred),
            "pedantic" => Ok(LintProfile::Pedantic),
            _ => Err(UnknownLintProfile {
                requested: name.to_string(),
            }),
        }
    }
}

/// A lint profile name SpecForge does not define.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLintProfile {
    pub requested: String,
}

impl fmt::Display for UnknownLintProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Unknown lint profile '{}' (available: {})",
            self.requested,
            LINT_PROFILE_NAMES.join(", ")
        )
    }
}

impl std::error::Error for UnknownLintProfile {}

/// How a surface turns a project's diagnostics into what it reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticPolicy {
    /// Warnings are promoted to errors.
    pub strict: bool,
    /// Extra lint profiles: their diagnostics are added before strict
    /// promotes warnings.
    pub lint_profiles: Vec<LintProfile>,
}

impl DiagnosticPolicy {
    /// Strict or not, with no lint profile.
    pub fn strict(strict: bool) -> Self {
        DiagnosticPolicy {
            strict,
            lint_profiles: Vec::new(),
        }
    }

    /// What a surface reports under this policy: for each lint profile the
    /// policy names (once, in [`LintProfile::ALL`] order), the diagnostics
    /// `lint` answers for it are added; then strict promotes warnings, the
    /// added ones too. A profile's diagnostics are its caller's to compute
    /// (`specforge_ops::check` answers `Inferred` from the inference
    /// manifest); the policy reads no file.
    pub fn apply(
        &self,
        mut diagnostics: Vec<Diagnostic>,
        mut lint: impl FnMut(LintProfile) -> Vec<Diagnostic>,
    ) -> Vec<Diagnostic> {
        for profile in LintProfile::ALL {
            if self.lint_profiles.contains(&profile) {
                diagnostics.extend(lint(profile));
            }
        }
        self.promote(&mut diagnostics);
        diagnostics
    }

    /// Promote warnings to errors when strict: for findings that have no
    /// project-level lints (analysis passes).
    pub fn promote(&self, diagnostics: &mut [Diagnostic]) {
        if self.strict {
            for diagnostic in diagnostics {
                if diagnostic.severity == Severity::Warning {
                    diagnostic.severity = Severity::Error;
                }
            }
        }
    }
}
