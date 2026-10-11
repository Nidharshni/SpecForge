//! `specforge doctor`: one project health report, shared by the CLI command
//! and the MCP `specforge.doctor` tool so both surfaces say the same thing.
//!
//! The report is built from what a compile already produced (the loaded
//! declarations and the diagnostics) plus the project's lock file on disk:
//! the project's extensions, each with its status and enhancement count, enhancements grouped
//! by the entity kind they target, and one list of findings (extension
//! conflicts with a resolution suggestion, keywords that shadow an entity
//! kind, unsatisfied peers, extensions that failed to load, installed-binary
//! integrity, and whether the z3 solver is on PATH). Each problem is one
//! finding that says what it is about; the report's sections are filters of
//! that list ([`DoctorReport::about`]) and its verdict is [`DoctorReport::ok`].
//!
//! Registry credential health is the user's, not the project's, and lives
//! with the credential store (`specforge_registry_client::credential_health`):
//! `specforge doctor` reports it; the MCP tool, whose spec'd report
//! (`McpDoctorReport`) is about the project, does not.

use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Code, Diagnostic, DiagnosticData, Severity, codes};
use specforge_installed::Health;
use std::collections::BTreeMap;

use crate::extension::Status;
use crate::view::ProjectView;

/// Diagnostic codes that mean two contributions collide.
const CONFLICT_CODES: [Code; 3] = [codes::E026, codes::E057, codes::W018];

/// Codes that mean a name shadows a grammar-level construct: E013 (a project
/// entity ID is a structural keyword or an extension's kind keyword) and E026
/// (a kind keyword is registered twice, which is also a conflict).
const SHADOWING_CODES: [Code; 2] = [codes::E013, codes::E026];

/// Codes that mean `specforge.json` is not used as written: E069 (it can't
/// be read, isn't a JSON object, or has a mistyped key or item).
const CONFIG_CODES: [Code; 1] = [codes::E069];

/// Codes the compile reports a peer requirement by (ADR 0041): E027 (unsatisfied, or a cycle among
/// required peers) and E073 (a range that is not SemVer).
const PEER_CODES: [Code; 2] = [codes::E027, codes::E073];

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
    /// Whether the z3 SMT solver is on PATH.
    pub z3_available: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Ok,
    Warn,
    Error,
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

/// What a doctor finding is about: the report's sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
#[serde(tag = "about", rename_all = "snake_case")]
pub enum About {
    /// `specforge.json`: E069, or none at the root (`config_missing`).
    Config,
    /// `specforge.lock` exists and cannot be read (`lock_unreadable`).
    Lock,
    /// An installed binary does not match the lock.
    Binary { issue: BinaryIssue },
    /// An enabled extension the compile could not load (not a binary problem).
    Load,
    /// Two contributions collide (E026, E057, W018).
    Conflict,
    /// A name shadows a grammar-level construct (E013, E026).
    Shadowing { keyword: String },
    /// A peer requirement the compile reports unsatisfied (E027, E073).
    Peer,
    /// The z3 solver is not on PATH.
    Toolchain,
}

/// The parts of a report, to select findings by ([`DoctorReport::about`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Config,
    Lock,
    Binary,
    Load,
    Conflict,
    Shadowing,
    Peer,
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
            About::Conflict => Part::Conflict,
            About::Shadowing { .. } => Part::Shadowing,
            About::Peer => Part::Peer,
            About::Toolchain => Part::Toolchain,
        }
    }
}

impl DoctorReport {
    /// The report's verdict: no error-level finding. `specforge doctor`
    /// exits by it (with the user's credentials); `specforge.doctor`
    /// returns it as `ok`.
    pub fn ok(&self) -> bool {
        self.findings
            .iter()
            .all(|f| f.status != FindingStatus::Error)
    }

    /// Every installed binary is healthy, every enabled extension loaded
    /// and every peer requirement is met.
    pub fn extensions_ok(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|f| matches!(f.about.part(), Part::Binary | Part::Load | Part::Peer))
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

    /// The findings where two contributions collide: those about a conflict,
    /// and a shadowing that is also a collision (E026, a kind registered
    /// twice).
    pub fn conflicts(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| match f.about {
            About::Conflict => true,
            About::Shadowing { .. } => CONFLICT_CODES.iter().any(|code| code.matches(&f.code)),
            _ => false,
        })
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
            z3_available: self.z3_available,
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
    pub z3_available: bool,
    pub extensions: Vec<ExtensionHealth>,
    /// Entity enhancements keyed by the entity kind they target.
    pub enhancements: BTreeMap<String, Vec<EnhancementEntry>>,
    /// The `check` of each conflicting finding.
    pub conflicts: Vec<String>,
    pub findings: Vec<Finding>,
}

/// The health report of the project the view was compiled from: its
/// loaded declarations, the diagnostics its surface reports, and, with a
/// root, its lock and installed binaries. Without a root the installation
/// checks are skipped (`installed_count: 0`).
pub fn diagnose(view: &ProjectView) -> DoctorReport {
    diagnose_with(view, z3_on_path())
}

/// [`diagnose`] with the z3 probe supplied, so tests do not depend on PATH.
pub fn diagnose_with(view: &ProjectView, z3_available: bool) -> DoctorReport {
    let declarations = view.registries().declarations();
    let diagnostics = view.reported();

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
                    edge_types: enhancement
                        .edge_types
                        .iter()
                        .map(|e| e.label.clone())
                        .collect(),
                    verify_kinds: enhancement.verify_kinds.clone(),
                });
        }
    }

    let mut findings = Vec::new();

    // The config itself: a `specforge.json` the compile could not use as
    // written (E069, an error: `check` fails on it, so doctor does too),
    // or none at the project root (a warning: the default config is a
    // valid project, but rarely the one meant).
    for diag in diagnostics
        .iter()
        .filter(|d| CONFIG_CODES.iter().any(|code| d.is(*code)))
    {
        findings.push(Finding {
            about: About::Config,
            check: diag.message.clone(),
            status: match diag.severity {
                Severity::Error => FindingStatus::Error,
                _ => FindingStatus::Warn,
            },
            code: diag.code.clone(),
            remediation: remediation(diag, || format!("run `specforge explain {}`", diag.code)),
        });
    }
    if let Some(root) = view.root()
        && !view.env().config_found
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

    // Extensions the compile could not load: `check` fails on them, so
    // doctor does too. A missing or changed binary is its binary finding
    // above.
    for failure in inventory
        .rows()
        .iter()
        .flat_map(|extension| extension.failures())
    {
        if failure.problem.is_module_health() {
            continue;
        }
        let diag = &failure.diagnostic;
        findings.push(Finding {
            about: About::Load,
            check: diag.message.clone(),
            status: match diag.severity {
                Severity::Error => FindingStatus::Error,
                _ => FindingStatus::Warn,
            },
            code: diag.code.clone(),
            remediation: remediation(diag, || format!("run `specforge explain {}`", diag.code)),
        });
    }

    // Conflicts and shadowed keywords the compile reported. A kind registered
    // twice (E026) that names the keyword it shadows is one finding, about
    // the shadowing.
    for diag in &diagnostics {
        let conflict = CONFLICT_CODES.iter().any(|code| diag.is(*code));
        let shadowing = SHADOWING_CODES.iter().any(|code| diag.is(*code));
        if !conflict && !shadowing {
            continue;
        }
        // The keyword is the diagnostic's data, not a quoted word of its
        // message.
        let about = match diag.data.as_deref() {
            Some(DiagnosticData::ShadowedKeyword { keyword }) if shadowing => About::Shadowing {
                keyword: keyword.clone(),
            },
            _ => About::Conflict,
        };
        findings.push(Finding {
            about,
            check: diag.message.clone(),
            status: match diag.severity {
                Severity::Error => FindingStatus::Error,
                _ => FindingStatus::Warn,
            },
            code: diag.code.clone(),
            remediation: remediation(diag, || {
                format!(
                    "uninstall or reconfigure one of the conflicting extensions \
                     (`specforge explain {}`)",
                    diag.code
                )
            }),
        });
    }

    // The peer requirements the compile reports unsatisfied: one rule, so doctor and
    // check cannot disagree (ADR 0041).
    for diag in diagnostics
        .iter()
        .filter(|d| PEER_CODES.iter().any(|code| d.is(*code)))
    {
        findings.push(Finding {
            about: About::Peer,
            check: diag.message.clone(),
            status: FindingStatus::Error,
            code: diag.code.clone(),
            remediation: remediation(diag, || format!("run `specforge explain {}`", diag.code)),
        });
    }

    if !z3_available {
        findings.push(Finding {
            about: About::Toolchain,
            check: "z3 on PATH".into(),
            status: FindingStatus::Warn,
            code: "z3_missing".into(),
            remediation: "install z3; without it `specforge analyze --prove` skips SMT checks \
                          (W098)"
                .into(),
        });
    }

    DoctorReport {
        extensions,
        enhancements,
        installed_count: view.lock().entries().len(),
        z3_available,
        findings,
    }
}

fn z3_on_path() -> bool {
    std::process::Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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
    use crate::view::testing::Fixture;
    use specforge_test_macros::test as specforge_test;

    fn diag(code: &str, message: &str, suggestion: Option<&str>) -> Diagnostic {
        let mut diagnostic = Diagnostic::untyped(code, Severity::Error, message);
        diagnostic.suggestion = suggestion.map(String::from);
        diagnostic
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor detects shadowed grammar-level constructs"
    )]
    fn an_e026_with_a_keyword_is_one_shadowing_finding() {
        // What the registry build reports for a kind two extensions declare,
        // worded so that no quoted word of it is the keyword: only the data
        // names it.
        let mut e026 = diag(
            "E026",
            "extension 'acme': its entity kind is registered by another extension",
            Some("choose a different keyword for this entity kind"),
        );
        e026.data = Some(Box::new(DiagnosticData::ShadowedKeyword {
            keyword: "memo".into(),
        }));
        let diagnostics = [e026];

        let fixture = Fixture::new().reporting(diagnostics.to_vec());
        let report = diagnose_with(&fixture.view(), true);

        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
        let finding = &report.findings[0];
        assert_eq!(
            finding.about,
            About::Shadowing {
                keyword: "memo".into()
            }
        );
        assert_eq!(finding.code, "E026");
        assert_eq!(
            finding.remediation,
            "choose a different keyword for this entity kind"
        );
        assert_eq!(report.about(Part::Conflict).count(), 0);
        assert!(!report.ok());
    }

    #[test]
    fn a_non_shadowing_conflict_is_listed_but_not_shadowed() {
        let diagnostics = [
            diag("W018", "edge type 'uses' declared twice", None),
            diag("W001", "unrelated", None),
        ];

        let fixture = Fixture::new().reporting(diagnostics.to_vec());
        let report = diagnose_with(&fixture.view(), true);

        let conflicts: Vec<&Finding> = report.about(Part::Conflict).collect();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(report.about(Part::Shadowing).count(), 0);
        // W018 offers no suggestion: the catalogue's explanation stands in.
        assert!(
            conflicts[0]
                .remediation
                .starts_with("Two extensions register an edge type with the same label"),
            "{}",
            conflicts[0].remediation
        );
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

        let report = diagnose_with(&view, true);

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

        let report = diagnose_with(&fixture.view(), true);

        let names: Vec<&str> = report.extensions.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["@a/a", "@m/m", "@z/z"]);
    }

    #[test]
    fn doctor_lists_load_failures_by_extension_name() {
        use specforge_installed::LoadProblem;
        let fixture = Fixture::new().enabled(vec![
            failing_entry("@z/z", LoadProblem::NotInstalled),
            failing_entry("@a/a", LoadProblem::NotInstalled),
        ]);

        let report = diagnose_with(&fixture.view(), true);

        let checks: Vec<&str> = report.about(Part::Load).map(|f| f.check.as_str()).collect();
        assert_eq!(checks, ["@a/a did not load", "@z/z did not load"]);
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

        let report = diagnose_with(&fixture.view(), true);

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
        let report = diagnose_with(&fixture.view(), true);

        assert_eq!(report.about(Part::Conflict).count(), 0);
        let shadowed: Vec<&Finding> = report.about(Part::Shadowing).collect();
        assert_eq!(
            shadowed[0].about,
            About::Shadowing {
                keyword: "behavior".into()
            }
        );
        assert_eq!(shadowed[0].remediation, "rename the entity");
        assert_eq!(report.findings[0].code, "E013");
    }

    #[test]
    fn a_missing_z3_is_a_warning_not_an_error() {
        let fixture = Fixture::new();
        let report = diagnose_with(&fixture.view(), false);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "z3_missing");
        assert!(report.ok());
        assert_eq!(report.cache_status(), CacheStatus::Ok);
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "doctor reads the diagnostics its view reports"
    )]
    fn doctor_reads_the_diagnostics_its_view_reports() {
        let e028 = diag("E028", "extension '@acme/x' is not installed", None);
        let failing = Fixture::new().enabled(vec![failed(
            "@acme/x",
            specforge_installed::LoadProblem::NotInstalled,
            e028,
        )]);
        let report = diagnose_with(&failing.view(), true);
        let failures: Vec<&str> = report.about(Part::Load).map(|f| f.code.as_str()).collect();
        assert_eq!(failures, ["E028"]);
        assert!(!report.ok());

        let clean = Fixture::new();
        let report = diagnose_with(&clean.view(), true);
        assert_eq!(report.about(Part::Load).count(), 0);
        assert!(report.ok());
    }

    #[test]
    fn a_rootless_view_skips_the_installation_checks() {
        let fixture = Fixture::new()
            .declarations(vec![Fixture::declaration("@acme/loaded", "2.0.0")])
            .lock(&[("@acme/locked", "1.0.0", "registry")]);

        let report = diagnose_with(&fixture.rootless_view(), true);

        assert_eq!(report.installed_count, 0);
        assert_eq!(report.cache_status(), CacheStatus::Ok);
        let names: Vec<&str> = report.extensions.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["@acme/loaded"], "no lock is read without a root");
        assert_eq!(report.extensions[0].status, Status::NotConfigured);

        // Rooted, the lock is read and its binary checked (missing here).
        let report = diagnose_with(&fixture.view(), true);
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

        let report = diagnose_with(&fixture.view(), true);

        assert_eq!(
            finding_codes(&report),
            [(CONFIG_MISSING, FindingStatus::Warn)]
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
        assert!(finding_codes(&diagnose_with(&found.view(), true)).is_empty());
    }

    #[specforge_test(
        behavior = "run_doctor_check",
        verify = "doctor in a directory without specforge.json reports config_missing as a warning"
    )]
    fn a_rootless_view_has_no_config_missing_finding() {
        let fixture = Fixture::new().without_config_file();

        let report = diagnose_with(&fixture.rootless_view(), true);

        assert!(finding_codes(&report).is_empty(), "{:?}", report.findings);
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
                root: dir.path(),
                source: crate::extension::Source::Local(blob),
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
        let compiled =
            specforge_project::CompiledProject::compile(dir.path(), Some(runtime.clone()));

        let report = diagnose_with(&ProjectView::of(&compiled), true);

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
                root: dir.path(),
                source: crate::extension::Source::Local(command.into()),
                allow_unsigned: false,
                trust: crate::extension::Trust::Refuse,
                dry_run: false,
            },
            &crate::registry::Unconfigured("add"),
            runtime.as_ref(),
        )
        .unwrap();
        let compiled = specforge_project::CompiledProject::compile(dir.path(), Some(runtime));
        let report = diagnose_with(&ProjectView::of(&compiled), true);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    #[specforge_test(
        behavior = "provide_mcp_doctor_tool",
        verify = "specforge.doctor reports an unusable specforge.json (E069) as a finding"
    )]
    fn an_unusable_config_is_an_error_finding() {
        let e069 = diag(
            "E069",
            "specforge.json can't be used: ./specforge.json is not valid JSON: expected value at line 1 column 41; no extension is loaded",
            Some("fix specforge.json; `specforge explain E069` says what it must be"),
        );
        let fixture = Fixture::new().reporting(vec![e069]);

        let report = diagnose_with(&fixture.view(), true);

        assert_eq!(finding_codes(&report), [("E069", FindingStatus::Error)]);
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
}
