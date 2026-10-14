//! `specforge doctor`: one project health report, shared by the CLI command
//! and the MCP `specforge.doctor` tool so both surfaces say the same thing.
//!
//! The report is built from what a compile already produced (the loaded
//! declarations and the environment's diagnostics) plus the project's lock
//! file on disk: the project's extensions, each with its status and
//! enhancement count, enhancements grouped by the entity kind they target,
//! and one list of findings. Every diagnostic the environment reports before
//! any source is one finding, about the part of the setup the environment
//! says it is about ([`specforge_project::SetupPart`]); doctor reads no
//! diagnostic code to decide that. Beside them: the disk checks only doctor
//! makes (the config file, the lock, installed binaries), the one source
//! diagnostic about the setup (E013, an entity id that is a keyword) and
//! whether the SMT solver `analyze --prove` asks runs here (its W098). The report's sections are filters of
//! that list ([`DoctorReport::about`]) and its verdict is
//! [`DoctorReport::ok`].
//!
//! Registry credential health is the user's, not the project's, and lives
//! with the credential store (`specforge_registry_client::credential_health`):
//! `specforge doctor` reports it; the MCP tool, whose spec'd report
//! (`McpDoctorReport`) is about the project, does not.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticData, Severity, codes};
use specforge_installed::Health;
use specforge_project::{Environment, SetupPart};
use std::collections::BTreeMap;

use crate::extension::Status;
use crate::solver::{Availability, Solver};
use crate::view::ProjectView;

/// The finding code of a project root without `specforge.json`.
const CONFIG_MISSING: &str = "config_missing";

/// The finding code of a `specforge.lock` that exists but cannot be read
/// (its check names the diagnostic, E033).
const LOCK_UNREADABLE: &str = "lock_unreadable";

/// Everything `specforge doctor` reports about a project.
#[derive(Debug, Clone)]
pub struct DoctorReport {
    /// Every extension of the project: its inventory, by name.
    pub extensions: Vec<ExtensionHealth>,
    /// Entity enhancements keyed by the entity kind they target (sorted).
    pub enhancements: BTreeMap<String, Vec<EnhancementEntry>>,
    /// Lock entries whose binaries were checked.
    pub installed_count: usize,
    /// Every problem, once, in report order, each saying what it is about.
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, Shape)]
pub struct ExtensionHealth {
    pub name: String,
    /// The version the extensions listing gives it; none when it has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Where it comes from, as the extensions listing names it: `builtin`,
    /// the lock entry's source, `file:<path>` for a `.wasm` file entry, or
    /// `unknown`.
    pub source: String,
    /// Whether it is part of the compiled project, as the listing says.
    pub status: Status,
    pub enhancement_count: usize,
}

#[derive(Debug, Clone, Serialize, Shape)]
pub struct EnhancementEntry {
    /// The extension contributing the enhancement.
    pub extension: String,
    pub fields: Vec<String>,
    pub edge_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_kinds: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BinaryIssue {
    MissingBinary {
        name: String,
    },
    StaleHash {
        name: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    Ok,
    Stale,
}

/// A finding's status: its diagnostic's severity (the disk checks' own are
/// error or warn).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Error,
    Warn,
    Info,
}

impl FindingStatus {
    /// The status of a finding about a diagnostic of `severity`.
    pub fn of(severity: Severity) -> Self {
        match severity {
            Severity::Error => FindingStatus::Error,
            Severity::Warning => FindingStatus::Warn,
            Severity::Info => FindingStatus::Info,
        }
    }
}

#[derive(Debug, Clone, Serialize, Shape)]
pub struct Finding {
    /// What the finding is about, with what that part knows.
    #[serde(flatten)]
    pub about: About,
    pub check: String,
    pub status: FindingStatus,
    pub code: String,
    pub remediation: String,
}

impl Finding {
    /// The finding `diagnostic` is, about `about`: its message, code and
    /// severity; its own suggestion, else the catalogue's explanation, else
    /// how to read more.
    fn of(about: About, diagnostic: &Diagnostic) -> Finding {
        let code = &diagnostic.code;
        let fallback = match about {
            About::Conflict { .. } => format!(
                "uninstall or reconfigure one of the conflicting extensions \
                 (`specforge explain {code}`)"
            ),
            _ => format!("run `specforge explain {code}`"),
        };
        Finding {
            status: FindingStatus::of(diagnostic.severity),
            check: diagnostic.message.clone(),
            code: code.clone(),
            remediation: remediation(diagnostic, || fallback),
            about,
        }
    }
}

/// What a doctor finding is about: the report's sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(tag = "about", rename_all = "snake_case")]
pub enum About {
    /// `specforge.json`: E069, or none at the root (`config_missing`).
    Config,
    /// `specforge.lock` exists and cannot be read (`lock_unreadable`; E033
    /// without a root).
    Lock,
    /// An installed binary does not match the lock.
    Binary { issue: BinaryIssue },
    /// One `extensions` entry: why it did not load (not a binary problem),
    /// or what it reported though it loaded (W149, W153, W138).
    Load,
    /// No extension loaded (I002).
    StructuralOnly,
    /// A configured provider (W118).
    Providers,
    /// One extension declaration on its own (E030, W021, W145).
    Declaration,
    /// A peer requirement the compile reports unsatisfied (E027, E073).
    Peer,
    /// What the registries hold (W019, W021, I004, W017).
    Registry,
    /// A declared rule (W150, W147, W112).
    Rule,
    /// An extension's surfaces (E055).
    Surface,
    /// Two contributions claim one name (E026, W018, W023, E039, E057); a
    /// kind keyword registered twice names the keyword it shadows.
    Conflict {
        #[serde(skip_serializing_if = "Option::is_none")]
        keyword: Option<String>,
    },
    /// An entity id that is a structural keyword or an extension's kind
    /// keyword (E013).
    Shadowing { keyword: String },
    /// The SMT solver `specforge analyze --prove` asks does not run here: the
    /// W098 it reports.
    Toolchain,
}

/// The parts of a report, to select findings by ([`DoctorReport::about`]):
/// one per [`About`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Config,
    Lock,
    Binary,
    Load,
    StructuralOnly,
    Providers,
    Declaration,
    Peer,
    Registry,
    Rule,
    Surface,
    Conflict,
    Shadowing,
    Toolchain,
}

impl About {
    /// The part of the report this finding belongs to.
    pub fn part(&self) -> Part {
        match self {
            About::Config => Part::Config,
            About::Lock => Part::Lock,
            About::Binary { .. } => Part::Binary,
            About::Load => Part::Load,
            About::StructuralOnly => Part::StructuralOnly,
            About::Providers => Part::Providers,
            About::Declaration => Part::Declaration,
            About::Peer => Part::Peer,
            About::Registry => Part::Registry,
            About::Rule => Part::Rule,
            About::Surface => Part::Surface,
            About::Conflict { .. } => Part::Conflict,
            About::Shadowing { .. } => Part::Shadowing,
            About::Toolchain => Part::Toolchain,
        }
    }

    /// The keyword a shadowing, or a kind conflict, names.
    pub fn keyword(&self) -> Option<&str> {
        match self {
            About::Shadowing { keyword } => Some(keyword),
            About::Conflict { keyword } => keyword.as_deref(),
            _ => None,
        }
    }
}

impl DoctorReport {
    /// The report's verdict: no error-level finding. `specforge doctor`
    /// exits by it (with the user's credentials); `specforge.doctor`
    /// returns it as `ok`. Every error the environment reports, E013, a
    /// binary that does not match the lock and an unreadable lock fail it.
    pub fn ok(&self) -> bool {
        self.findings
            .iter()
            .all(|f| f.status != FindingStatus::Error)
    }

    /// No error-level finding about an installed binary, an extension load
    /// or a peer requirement.
    pub fn extensions_ok(&self) -> bool {
        !self.findings.iter().any(|f| {
            f.status == FindingStatus::Error
                && matches!(f.about.part(), Part::Binary | Part::Load | Part::Peer)
        })
    }

    /// `stale` when an installed binary is missing or its hash drifted.
    pub fn cache_status(&self) -> CacheStatus {
        match self.about(Part::Binary).next() {
            Some(_) => CacheStatus::Stale,
            None => CacheStatus::Ok,
        }
    }

    /// The findings about one part, in report order.
    pub fn about(&self, part: Part) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.about.part() == part)
    }

    /// Whether the SMT solver runs here: no toolchain finding says otherwise.
    pub fn z3_available(&self) -> bool {
        self.about(Part::Toolchain).next().is_none()
    }

    /// The findings where two contributions collide.
    pub fn conflicts(&self) -> impl Iterator<Item = &Finding> {
        self.about(Part::Conflict)
    }

    /// The findings that name a shadowed keyword: E013's, and a kind
    /// keyword registered twice (E026).
    pub fn shadowed(&self) -> impl Iterator<Item = (&str, &Finding)> {
        self.findings
            .iter()
            .filter_map(|f| f.about.keyword().map(|keyword| (keyword, f)))
    }

    /// The one document both surfaces return: `{ok, extensions_ok,
    /// cache_status, installed_count, z3_available, extensions,
    /// enhancements, conflicts: [message…], findings: [...]}`.
    pub fn document(&self) -> DoctorDocument {
        DoctorDocument {
            ok: self.ok(),
            extensions_ok: self.extensions_ok(),
            cache_status: self.cache_status(),
            installed_count: self.installed_count,
            z3_available: self.z3_available(),
            extensions: self.extensions.clone(),
            enhancements: self.enhancements.clone(),
            conflicts: self.conflicts().map(|f| f.check.clone()).collect(),
            findings: self.findings.clone(),
        }
    }
}

/// What `specforge doctor --format json` and `specforge.doctor` answer with
/// (`McpDoctorReport`); the CLI prints it with the user's credential health
/// beside it (ADR 0029 D3a, ADR 0052 D8).
#[derive(Debug, Clone, Serialize, Shape)]
pub struct DoctorDocument {
    /// No error-level finding.
    pub ok: bool,
    pub extensions_ok: bool,
    pub cache_status: CacheStatus,
    pub installed_count: usize,
    /// Whether the SMT solver z3 runs here: false exactly when a toolchain
    /// finding says why not.
    pub z3_available: bool,
    pub extensions: Vec<ExtensionHealth>,
    /// Entity enhancements keyed by the entity kind they target.
    pub enhancements: BTreeMap<String, Vec<EnhancementEntry>>,
    /// The `check` of each conflicting finding.
    pub conflicts: Vec<String>,
    pub findings: Vec<Finding>,
}

/// The health report of the project the view was compiled from: its loaded
/// declarations, the diagnostics its environment reports, with a root its lock
/// and installed binaries, and whether `solver` runs here.
pub fn diagnose(view: &ProjectView, solver: &dyn Solver) -> DoctorReport {
    let declarations = view.registries().declarations();
    let env = view.env();

    // The project's extensions: the listing's rows, in its order.
    let inventory = crate::extension::inventory(view);
    let extensions: Vec<ExtensionHealth> = inventory
        .rows()
        .iter()
        .map(|extension| ExtensionHealth {
            name: extension.name.to_string(),
            version: extension.version().map(str::to_string),
            source: extension.origin.source(),
            status: extension.status,
            enhancement_count: extension.declaration.map_or(0, |d| d.enhancements.len()),
        })
        .collect();

    let mut enhancements: BTreeMap<String, Vec<EnhancementEntry>> = BTreeMap::new();
    for declaration in declarations {
        for enhancement in &declaration.enhancements {
            enhancements
                .entry(enhancement.target_kind.clone())
                .or_default()
                .push(EnhancementEntry {
                    extension: declaration.name().to_string(),
                    fields: enhancement.fields.iter().map(|f| f.name.clone()).collect(),
                    edge_types: written_edge_types(&enhancement.fields),
                    verify_kinds: enhancement.verify_kinds.clone(),
                });
        }
    }

    let mut findings = Vec::new();

    // 1. The disk, which only doctor checks (with a root).
    //
    // No `specforge.json` at the project root (a warning: the default config
    // is a valid project, but rarely the one meant).
    if let Some(root) = view.root()
        && !env.config_found()
    {
        findings.push(Finding {
            about: About::Config,
            check: format!("specforge.json at {}", root.display()),
            status: FindingStatus::Warn,
            code: CONFIG_MISSING.into(),
            remediation: "run `specforge init` here, or pass --path to the project root; without \
                          specforge.json the project has the default config and no extension"
                .into(),
        });
    }

    // A lock file that cannot be used: nothing is known to be installed.
    if let Some(problem) = view.lock().problem() {
        findings.push(Finding {
            about: About::Lock,
            check: format!("{} [{}]", problem.message, problem.code),
            status: FindingStatus::Error,
            code: LOCK_UNREADABLE.into(),
            remediation: remediation(problem, || {
                format!("run `specforge explain {}`", problem.code)
            }),
        });
    }

    // Installed binaries against the lock file. A missing or changed binary
    // is one finding with the command that reinstalls it; the load failure
    // it causes is not listed again below.
    let installed = view.installed();
    let reinstall = |name: &str| format!("run `{}` to reinstall it", installed.reinstall(name));
    for status in installed.health() {
        let (issue, check, code, name) = match status {
            Health::MissingModule { name } => (
                BinaryIssue::MissingBinary { name: name.clone() },
                format!("extension {name}"),
                "missing_binary",
                name,
            ),
            Health::Changed {
                name,
                locked: expected,
                actual,
            } => (
                BinaryIssue::StaleHash {
                    name: name.clone(),
                    expected: expected.clone(),
                    actual: actual.clone(),
                },
                format!("extension {name}: lock expects {expected}, found {actual}"),
                "stale_hash",
                name,
            ),
        };
        findings.push(Finding {
            about: About::Binary { issue },
            check,
            status: FindingStatus::Error,
            code: code.into(),
            remediation: reinstall(&name),
        });
    }

    // 2. The setup, as `check` reports it before any source: one finding per
    // environment diagnostic, in `check`'s order, about the part the
    // environment says it is about; except what step 1 already states.
    let rooted = view.root().is_some();
    for setup in env.setup_diagnostics() {
        if rooted && stated_on_disk(env, setup.part) {
            continue;
        }
        findings.push(Finding::of(
            about(setup.part, setup.diagnostic),
            setup.diagnostic,
        ));
    }

    // 3. The one source diagnostic about the setup: an entity id a keyword
    // shadows (E013).
    for diagnostic in view.reported().iter().filter(|d| d.is(codes::E013)) {
        let keyword = keyword_of(diagnostic).unwrap_or_default();
        findings.push(Finding::of(About::Shadowing { keyword }, diagnostic));
    }

    // 4. The toolchain: the solver `specforge analyze --prove` asks.
    if let Availability::Unavailable(diagnostic) = solver.availability() {
        findings.push(Finding::of(About::Toolchain, &diagnostic));
    }

    DoctorReport {
        extensions,
        enhancements,
        installed_count: view.lock().entries().len(),
        findings,
    }
}

/// What a finding about a diagnostic of `part` is about: one arm per part,
/// so a new part is a compile error here.
fn about(part: SetupPart, diagnostic: &Diagnostic) -> About {
    match part {
        SetupPart::Config => About::Config,
        SetupPart::Lock => About::Lock,
        SetupPart::Entry(_) => About::Load,
        SetupPart::StructuralOnly => About::StructuralOnly,
        SetupPart::Providers => About::Providers,
        SetupPart::Declaration => About::Declaration,
        SetupPart::Peers => About::Peer,
        SetupPart::Registries => About::Registry,
        SetupPart::Rules => About::Rule,
        SetupPart::Surfaces => About::Surface,
        SetupPart::Collision => About::Conflict {
            keyword: keyword_of(diagnostic),
        },
    }
}

/// Whether a disk check states this diagnostic already (with a root, ADR
/// 0028): the lock's E033 is the lock finding, and an entry whose module is
/// missing or changed is its binary finding.
fn stated_on_disk(env: &Environment, part: SetupPart) -> bool {
    match part {
        SetupPart::Lock => true,
        SetupPart::Entry(i) => env.enabled()[i]
            .failure
            .as_ref()
            .is_some_and(|failure| failure.problem.is_module_health()),
        _ => false,
    }
}

/// The keyword a diagnostic's data names (E013, E026): never a word of its
/// message.
fn keyword_of(diagnostic: &Diagnostic) -> Option<String> {
    match diagnostic.data.as_deref() {
        Some(DiagnosticData::ShadowedKeyword { keyword }) => Some(keyword.clone()),
        _ => None,
    }
}

/// The distinct edge types the fields write, in field order.
fn written_edge_types(fields: &[specforge_protocol_types::FieldDescriptor]) -> Vec<String> {
    let mut edge_types: Vec<String> = Vec::new();
    for edge in fields.iter().filter_map(|f| f.edge.as_deref()) {
        if !edge_types.iter().any(|seen| seen == edge) {
            edge_types.push(edge.to_string());
        }
    }
    edge_types
}

/// How to fix what `diag` reports: its own suggestion, else the
/// catalogue's explanation of its code, else `fallback` (a code the
/// catalogue doesn't have).
fn remediation(diag: &Diagnostic, fallback: impl FnOnce() -> String) -> String {
    diag.suggestion
        .clone()
        .or_else(|| {
            specforge_diagnostics::describes(&diag.code, diag.origin())
                .map(|e| e.explanation.to_string())
        })
        .unwrap_or_else(fallback)
}

#[cfg(test)]
mod tests;
