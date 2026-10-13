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
mod tests {
    use super::*;
    use crate::testing::ScriptedSolver;
    use crate::view::testing::Fixture;
    use specforge_test_macros::test as specforge_test;

    fn diag(code: &str, message: &str, suggestion: Option<&str>) -> Diagnostic {
        let mut diagnostic = Diagnostic::untyped(code, Severity::Error, message);
        diagnostic.suggestion = suggestion.map(String::from);
        diagnostic
    }

    /// A declaration of `name` that contributes the entity kind `keyword`.
    fn declaring_kind(name: &str, keyword: &str) -> specforge_protocol_types::ExtensionDeclaration {
        let mut declaration = Fixture::declaration(name, "1.0.0");
        declaration.entities = vec![specforge_protocol_types::EntityKindDescriptor {
            name: keyword.into(),
            keyword: Some(keyword.into()),
            ..Default::default()
        }];
        declaration
    }

    /// A declaration of `name` that contributes the edge type `label`.
    fn declaring_edge(name: &str, label: &str) -> specforge_protocol_types::ExtensionDeclaration {
        let mut declaration = Fixture::declaration(name, "1.0.0");
        declaration.edges = vec![specforge_protocol_types::EdgeTypeDescriptor {
            label: label.into(),
            ..Default::default()
        }];
        declaration
    }

    #[test]
    fn enhancements_list_the_edge_types_their_fields_write() {
        use specforge_protocol_types::{EntityEnhancementDescriptor, FieldDescriptor};
        let field = |name: &str, edge: Option<&str>| FieldDescriptor {
            name: name.into(),
            edge: edge.map(str::to_string),
            ..Default::default()
        };
        let mut declaration = Fixture::declaration("@acme/a", "1.0.0");
        declaration.enhancements = vec![EntityEnhancementDescriptor {
            target_kind: "behavior".into(),
            source_extension: "@acme/a".into(),
            fields: vec![
                field("requires", Some("Requires")),
                field("note", None),
                field("also_requires", Some("Requires")),
                field("ensures", Some("Ensures")),
            ],
            ..Default::default()
        }];
        let fixture = Fixture::new().declarations(vec![declaration]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        let entry = &report.enhancements["behavior"][0];
        assert_eq!(entry.edge_types, ["Requires", "Ensures"]);
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor detects shadowed grammar-level constructs"
    )]
    fn a_kind_registered_twice_is_one_conflict_naming_the_keyword() {
        // What the registry build reports for a kind two extensions declare:
        // the keyword is the diagnostic's data, not a word of its message.
        let fixture = Fixture::new().declarations(vec![
            declaring_kind("@acme/a", "memo"),
            declaring_kind("@acme/b", "memo"),
        ]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        let finding = &report.findings[0];
        assert_eq!(
            finding.about,
            About::Conflict {
                keyword: Some("memo".into())
            }
        );
        assert_eq!(finding.code, "E026");
        assert!(
            finding
                .remediation
                .ends_with("Rename the conflicting kind keyword."),
            "the catalogued explanation: {}",
            finding.remediation
        );
        assert_eq!(
            report.shadowed().map(|(k, _)| k).collect::<Vec<_>>(),
            ["memo"]
        );
        assert_eq!(report.conflicts().count(), 1);
        assert_eq!(report.about(Part::Shadowing).count(), 0);
        assert!(!report.ok());
    }

    #[test]
    fn a_non_shadowing_conflict_is_listed_but_not_shadowed() {
        let fixture = Fixture::new()
            .declarations(vec![
                declaring_edge("@acme/a", "uses"),
                declaring_edge("@acme/b", "uses"),
            ])
            .reporting(vec![diag("W001", "unrelated", None)]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        let conflicts: Vec<&Finding> = report.conflicts().collect();
        assert_eq!(conflicts.len(), 1, "{:?}", report.findings);
        assert_eq!(conflicts[0].code, "W018");
        assert_eq!(conflicts[0].about, About::Conflict { keyword: None });
        assert_eq!(report.shadowed().count(), 0);
        // W018 offers no suggestion: the catalogue's explanation stands in.
        assert!(
            conflicts[0]
                .remediation
                .starts_with("Two extensions register an edge type with the same label"),
            "{}",
            conflicts[0].remediation
        );
        // A source diagnostic that is not E013 is no finding.
        assert!(report.findings.iter().all(|f| f.code != "W001"));
    }

    /// An entry the load could not load, as the environment records it.
    fn failed(
        entry: &str,
        problem: specforge_installed::LoadProblem,
        diagnostic: Diagnostic,
    ) -> specforge_project::EnabledExtension {
        specforge_project::EnabledExtension {
            failure: Some(specforge_installed::LoadFailure {
                problem,
                diagnostic,
            }),
            ..specforge_project::EnabledExtension::unloaded(entry)
        }
    }

    fn failing_entry(
        entry: &str,
        problem: specforge_installed::LoadProblem,
    ) -> specforge_project::EnabledExtension {
        failed(
            entry,
            problem,
            Diagnostic::new(codes::E028, format!("{entry} did not load")),
        )
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor lists the extensions the extensions listing lists, each with its version, source and status"
    )]
    fn doctor_lists_the_extensions_the_listing_lists() {
        use specforge_installed::LoadProblem;
        let fixture = Fixture::new()
            .enabled(vec![
                specforge_project::EnabledExtension::unloaded("@specforge/product"),
                specforge_project::EnabledExtension {
                    entry: "greet.wasm".into(),
                    name: "@sdk/greet".into(),
                    file: Some("greet.wasm".into()),
                    failure: None,
                    warnings: Vec::new(),
                },
                failing_entry("@acme/missing", LoadProblem::NotInstalled),
                failing_entry(
                    "@test/probe=nope.wasm",
                    LoadProblem::FileMissing {
                        path: "nope.wasm".into(),
                    },
                ),
            ])
            .declarations(vec![
                Fixture::declaration("@specforge/product", "1.0.0"),
                Fixture::declaration("@sdk/greet", "0.1.0"),
                Fixture::declaration("@acme/stray", "3.0.0"),
            ])
            .lock(&[
                ("@test/probe", "0.1.0", "local:/blobs/probe.wasm"),
                ("@m/m", "1.0.0", "registry"),
            ]);
        let view = fixture.view();

        let report = diagnose(&view, &ScriptedSolver::ready());

        let doctor: Vec<(&str, Option<&str>, &str, Status)> = report
            .extensions
            .iter()
            .map(|e| {
                (
                    e.name.as_str(),
                    e.version.as_deref(),
                    e.source.as_str(),
                    e.status,
                )
            })
            .collect();
        let listing = crate::extension::list(&view);
        let listed: Vec<(&str, Option<&str>, &str, Status)> = listing
            .extensions
            .iter()
            .map(|e| {
                (
                    e.name.as_str(),
                    e.version.as_deref(),
                    e.source.as_str(),
                    e.status,
                )
            })
            .collect();
        assert_eq!(doctor, listed);
        assert_eq!(
            doctor,
            [
                ("@acme/missing", None, "unknown", Status::NotLoaded),
                (
                    "@acme/stray",
                    Some("3.0.0"),
                    "unknown",
                    Status::NotConfigured
                ),
                ("@m/m", Some("1.0.0"), "registry", Status::NotConfigured),
                (
                    "@sdk/greet",
                    Some("0.1.0"),
                    "file:greet.wasm",
                    Status::Loaded
                ),
                (
                    "@specforge/product",
                    Some("1.0.0"),
                    "builtin",
                    Status::Loaded
                ),
                ("@test/probe", None, "file:nope.wasm", Status::NotLoaded),
            ]
        );
    }

    #[test]
    fn doctor_lists_extensions_by_name() {
        let fixture = Fixture::new()
            .config(&["@z/z", "@a/a"])
            .declarations(vec![
                Fixture::declaration("@z/z", "1.0.0"),
                Fixture::declaration("@a/a", "1.0.0"),
            ])
            .lock(&[("@m/m", "1.0.0", "registry")]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        let names: Vec<&str> = report.extensions.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["@a/a", "@m/m", "@z/z"]);
    }

    #[test]
    fn doctor_lists_load_failures_in_check_order() {
        use specforge_installed::LoadProblem;
        let fixture = Fixture::new().enabled(vec![
            failing_entry("@z/z", LoadProblem::NotInstalled),
            failing_entry("@a/a", LoadProblem::NotInstalled),
        ]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        let checks: Vec<&str> = report.about(Part::Load).map(|f| f.check.as_str()).collect();
        assert_eq!(checks, ["@z/z did not load", "@a/a did not load"]);
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "a finding without its own suggestion quotes the catalogued explanation"
    )]
    fn a_finding_without_a_suggestion_quotes_the_catalogue() {
        use specforge_installed::LoadProblem;
        let fixture = Fixture::new().enabled(vec![
            // No suggestion: the explanation of E028 is quoted.
            failed(
                "@acme/x",
                LoadProblem::NotInstalled,
                diag("E028", "extension '@acme/x' is not installed", None),
            ),
            // Its own suggestion wins.
            failed(
                "@acme/y",
                LoadProblem::NoDeclaration {
                    reason: "no handshake".into(),
                },
                diag(
                    "E028",
                    "protocol loading failed",
                    Some("run `specforge add @acme/y`"),
                ),
            ),
        ]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        let remedies: Vec<&str> = report
            .about(Part::Load)
            .map(|f| f.remediation.as_str())
            .collect();
        assert_eq!(remedies.len(), 2, "{remedies:?}");
        assert!(
            remedies[0].contains("confirm the extension is installed and up to date"),
            "E028's catalogued explanation: {}",
            remedies[0]
        );
        assert!(
            !remedies[0].contains("specforge explain"),
            "{}",
            remedies[0]
        );
        assert_eq!(remedies[1], "run `specforge add @acme/y`");
        let finding = report.findings.iter().find(|f| f.code == "E028").unwrap();
        assert_eq!(finding.remediation, remedies[0]);
    }

    #[test]
    fn an_entity_id_that_is_a_kind_keyword_is_shadowed_but_not_a_conflict() {
        let mut e013 = diag(
            "E013",
            "entity ID 'behavior' collides with a reserved keyword at project.spec",
            Some("rename the entity"),
        );
        e013.data = Some(Box::new(DiagnosticData::ShadowedKeyword {
            keyword: "behavior".into(),
        }));
        let diagnostics = [e013];

        let fixture = Fixture::new().reporting(diagnostics.to_vec());
        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        assert_eq!(report.about(Part::Conflict).count(), 0);
        let shadowed: Vec<&Finding> = report.about(Part::Shadowing).collect();
        assert_eq!(
            shadowed[0].about,
            About::Shadowing {
                keyword: "behavior".into()
            }
        );
        assert_eq!(shadowed[0].remediation, "rename the entity");
        assert!(report.findings.iter().any(|f| f.code == "E013"));
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor reports a solver that does not run as the W098 analyze --prove reports, as a toolchain warning"
    )]
    fn a_solver_that_does_not_run_is_the_w098_prove_reports_as_a_toolchain_warning() {
        let report = diagnose(&Fixture::new().view(), &ScriptedSolver::not_found());
        assert_eq!(
            finding_codes(&report),
            [("I002", FindingStatus::Info), ("W098", FindingStatus::Warn)]
        );
        let w098 = crate::solver::Z3::not_on_path();
        let toolchain: Vec<&Finding> = report.about(Part::Toolchain).collect();
        assert_eq!(toolchain.len(), 1);
        assert_eq!(toolchain[0].check, w098.message);
        assert_eq!(
            Some(toolchain[0].remediation.as_str()),
            w098.suggestion.as_deref()
        );
        assert!(report.ok() && !report.z3_available() && !report.document().z3_available);
        assert_eq!(report.cache_status(), CacheStatus::Ok);
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "doctor reads the setup diagnostics of its view's environment, and E013 from what its view reports"
    )]
    fn doctor_reads_the_diagnostics_its_view_reports() {
        let e028 = diag("E028", "extension '@acme/x' is not installed", None);
        let failing = Fixture::new().enabled(vec![failed(
            "@acme/x",
            specforge_installed::LoadProblem::NotInstalled,
            e028,
        )]);
        let report = diagnose(&failing.view(), &ScriptedSolver::ready());
        let failures: Vec<&str> = report.about(Part::Load).map(|f| f.code.as_str()).collect();
        assert_eq!(failures, ["E028"]);
        assert!(!report.ok());

        let clean = Fixture::new();
        let report = diagnose(&clean.view(), &ScriptedSolver::ready());
        assert_eq!(report.about(Part::Load).count(), 0);
        assert!(report.ok());
    }

    #[test]
    fn a_rootless_view_skips_the_installation_checks() {
        let fixture = Fixture::new()
            .declarations(vec![Fixture::declaration("@acme/loaded", "2.0.0")])
            .lock(&[("@acme/locked", "1.0.0", "registry")]);

        let report = diagnose(&fixture.rootless_view(), &ScriptedSolver::ready());

        assert_eq!(report.installed_count, 0);
        assert_eq!(report.cache_status(), CacheStatus::Ok);
        let names: Vec<&str> = report.extensions.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["@acme/loaded"], "no lock is read without a root");
        assert_eq!(report.extensions[0].status, Status::NotConfigured);

        // Rooted, the lock is read and its binary checked (missing here).
        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());
        assert_eq!(report.installed_count, 1);
        assert_eq!(report.cache_status(), CacheStatus::Stale);
    }

    fn finding_codes(report: &DoctorReport) -> Vec<(&str, FindingStatus)> {
        report
            .findings
            .iter()
            .map(|f| (f.code.as_str(), f.status))
            .collect()
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor in a directory without specforge.json reports config_missing as a warning"
    )]
    fn doctor_without_specforge_json_says_so() {
        let fixture = Fixture::new().without_config_file();

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        assert_eq!(
            finding_codes(&report),
            [
                (CONFIG_MISSING, FindingStatus::Warn),
                ("I002", FindingStatus::Info)
            ]
        );
        assert!(report.ok(), "a warning: doctor stays healthy");
        assert!(
            report.findings[0]
                .check
                .contains(&fixture.dir.path().display().to_string()),
            "{:?}",
            report.findings[0]
        );
        assert!(report.findings[0].remediation.contains("specforge init"));

        // A project with specforge.json gets no such finding.
        let found = Fixture::new();
        assert_eq!(
            finding_codes(&diagnose(&found.view(), &ScriptedSolver::ready())),
            [("I002", FindingStatus::Info)]
        );
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor in a directory without specforge.json reports config_missing as a warning"
    )]
    fn a_rootless_view_has_no_config_missing_finding() {
        let fixture = Fixture::new().without_config_file();

        let report = diagnose(&fixture.rootless_view(), &ScriptedSolver::ready());

        assert_eq!(
            finding_codes(&report),
            [("I002", FindingStatus::Info)],
            "{:?}",
            report.findings
        );
    }

    /// A project with `@sdk/greet` installed from a file in a second
    /// directory (returned with it: the lock names its path), whose module
    /// was then replaced by other bytes.
    fn project_with_a_changed_greet() -> (tempfile::TempDir, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name": "p", "version": "0.1.0", "extensions": []}"#,
        )
        .unwrap();
        let files = tempfile::tempdir().unwrap();
        let blob = files.path().join("greet.wasm");
        std::fs::write(&blob, crate::testing::GREET).unwrap();
        crate::extension::add(
            &crate::extension::AddRequest {
                root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions: vec![crate::extension::Source::Local(blob).into()],
                allow_unsigned: false,
                trust: crate::extension::Trust::Refuse,
                dry_run: false,
            },
            &crate::registry::Unconfigured("add"),
            &crate::testing::candidates(),
        )
        .unwrap();
        let module = dir
            .path()
            .join(".specforge/extensions/@sdk/greet/extension.wasm");
        let mut bytes = std::fs::read(&module).unwrap();
        bytes.extend_from_slice(b"changed after install");
        std::fs::write(module, bytes).unwrap();
        (dir, files)
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor reports a missing or changed installed binary once, with the remedy its load gives"
    )]
    fn doctor_reports_a_changed_binary_once() {
        let (dir, _files) = project_with_a_changed_greet();
        let runtime = std::sync::Arc::new(crate::testing::candidates());
        let compiled = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Some(runtime.clone()),
        );

        let report = diagnose(&ProjectView::of(&compiled), &ScriptedSolver::ready());

        let about_greet: Vec<(&str, &str)> = report
            .findings
            .iter()
            .filter(|f| f.check.contains("@sdk/greet"))
            .map(|f| (f.code.as_str(), f.remediation.as_str()))
            .collect();
        assert_eq!(about_greet.len(), 1, "{:?}", report.findings);
        let (code, remedy) = about_greet[0];
        assert_eq!(code, "stale_hash");
        assert!(
            remedy.starts_with("run `specforge add ") && remedy.ends_with(".wasm` to reinstall it"),
            "{remedy}"
        );
        // The binary is the one finding: the load failure it causes (E070)
        // is not listed again.
        assert_eq!(report.about(Part::Binary).count(), 1);
        assert_eq!(report.about(Part::Load).count(), 0);
        let greet = report
            .extensions
            .iter()
            .find(|e| e.name == "@sdk/greet")
            .unwrap();
        assert_eq!(greet.status, Status::NotLoaded);
        assert!(report.findings.iter().all(|f| f.code != "E070"));
        assert!(matches!(
            report.findings[0].about,
            About::Binary {
                issue: BinaryIssue::StaleHash { .. }
            }
        ));

        // Running the remedy reinstalls the pinned binary: doctor is clean.
        let command = remedy
            .strip_prefix("run `specforge add ")
            .and_then(|r| r.strip_suffix("` to reinstall it"))
            .unwrap();
        crate::extension::add(
            &crate::extension::AddRequest {
                root: &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
                extensions: vec![crate::extension::Source::Local(command.into()).into()],
                allow_unsigned: false,
                trust: crate::extension::Trust::Refuse,
                dry_run: false,
            },
            &crate::registry::Unconfigured("add"),
            runtime.as_ref(),
        )
        .unwrap();
        let compiled = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Some(runtime),
        );
        let report = diagnose(&ProjectView::of(&compiled), &ScriptedSolver::ready());
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    #[specforge_test(
        behavior = "provide_mcp_doctor_tool",
        verify = "specforge.doctor reports an unusable specforge.json (E069) as a finding"
    )]
    fn an_unusable_config_is_an_error_finding() {
        // What the environment reports for a specforge.json that is not JSON.
        let fixture =
            Fixture::new().config_problems(vec![specforge_common::ConfigProblem::NotJson {
                path: "./specforge.json".into(),
                error: "expected value at line 1 column 41".into(),
            }]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        assert_eq!(
            finding_codes(&report),
            [
                ("E069", FindingStatus::Error),
                ("I002", FindingStatus::Info)
            ]
        );
        assert!(!report.ok());
        assert_eq!(
            report.findings[0].remediation,
            "fix specforge.json; `specforge explain E069` says what it must be"
        );
        assert_eq!(report.findings[0].about, About::Config);
        assert_eq!(
            report.about(Part::Load).count(),
            0,
            "E069 is about the config, not an extension"
        );
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor reports every diagnostic the environment reports as one finding, with the part of the setup it is about and the diagnostic's severity"
    )]
    fn a_load_warning_is_a_finding_but_leaves_extensions_ok() {
        let fixture = Fixture::new()
            .enabled(vec![specforge_project::EnabledExtension {
                warnings: vec![Diagnostic::new(codes::W153, "its sandbox asks for more")],
                ..specforge_project::EnabledExtension::unloaded("@acme/x")
            }])
            .declarations(vec![Fixture::declaration("@acme/x", "1.0.0")]);

        let report = diagnose(&fixture.view(), &ScriptedSolver::ready());

        assert_eq!(finding_codes(&report), [("W153", FindingStatus::Warn)]);
        assert_eq!(report.findings[0].about, About::Load);
        assert!(report.ok() && report.extensions_ok());
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor reports a missing or changed installed binary once, with the remedy its load gives"
    )]
    fn without_a_root_a_missing_module_is_a_load_finding() {
        use specforge_installed::LoadProblem;
        let fixture = Fixture::new().enabled(vec![failing_entry(
            "@acme/gone",
            LoadProblem::ModuleMissing {
                path: "gone.wasm".into(),
            },
        )]);

        // With a root the lock's binary check states it (no lock entry here,
        // so nothing); without one nothing is checked on disk, so the
        // environment's own diagnostic is the finding.
        let report = diagnose(&fixture.rootless_view(), &ScriptedSolver::ready());

        assert_eq!(report.about(Part::Binary).count(), 0);
        let loads: Vec<&str> = report.about(Part::Load).map(|f| f.code.as_str()).collect();
        assert_eq!(loads, ["E028"]);
        assert!(!report.ok() && !report.extensions_ok());

        let rooted = diagnose(&fixture.view(), &ScriptedSolver::ready());
        assert_eq!(
            rooted.about(Part::Load).count(),
            0,
            "a module-health failure is its binary finding when there is a root"
        );
    }
}
