//! The host's coverage: how the recorded test report is read and what it
//! proves of the entity snapshot's entities (ADR 0019, ADR 0004 D2-f).
//!
//! Every surface that asks what an entity's recorded tests prove (stats,
//! the coverage view, inspect's coverage, query with coverage, the review,
//! the trace prompt's unverified entities, an extension command's
//! evidence) reads it here, so they cannot disagree. Who is testable and
//! what an entity declares are the snapshot's standing
//! (`ProjectView::entities`), which reads no report.

use crate::Environment;
use serde::Deserialize;
use specforge_graph::Graph;
use specforge_protocol_types::{EntityTests, TestResult, TestResults};
use specforge_registry::{EntitySnapshot, Standing};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// The recorded test report, at the project root: `collect` writes it,
/// `analyze` and the coverage views read it.
pub const REPORT_FILE: &str = "specforge-report.json";

/// The recorded test report (`specforge-report.json`): per entity id, the
/// tests `collect` recorded, each with the collector that recorded it. A
/// status outside the protocol's [`TestStatus`] vocabulary, or a test
/// without a name, makes it a report that cannot be used (E045, ADR 0069).
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct TestReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    pub results: std::collections::BTreeMap<String, ReportedEntity>,
}

/// The tests recorded for one entity.
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ReportedEntity {
    pub tests: Vec<ReportedTest>,
}

/// One recorded test: the collector's [`TestResult`] and the runner that
/// recorded it.
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ReportedTest {
    #[serde(flatten)]
    pub result: TestResult,
    /// The collector that recorded the test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
}

pub use specforge_coverage::{Status, Summary, Verdict};

/// The report as a pass receives it and the coverage rule reads it
/// (`PassInput::test_results`): each test's result without its runner.
impl From<&TestReport> for TestResults {
    fn from(report: &TestReport) -> Self {
        TestResults {
            runner: report.runner.clone(),
            results: report
                .results
                .iter()
                .map(|(id, entity)| {
                    let tests = entity.tests.iter().map(|t| t.result.clone()).collect();
                    (id.clone(), EntityTests { tests })
                })
                .collect(),
        }
    }
}

/// Why a test report could not be used. It says what happened, not what to
/// make of it: `specforge_ops` decides the kind of failure it is (ADR 0029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportError {
    /// The report is there (or was named) but could not be read: what the
    /// OS said.
    Unreadable {
        path: PathBuf,
        io: std::io::ErrorKind,
        detail: String,
    },
    /// The report is not a `specforge-report.json`: not UTF-8, or not the
    /// RES-15 shape.
    Malformed { path: PathBuf, detail: String },
}

impl ReportError {
    pub fn path(&self) -> &Path {
        match self {
            ReportError::Unreadable { path, .. } | ReportError::Malformed { path, .. } => path,
        }
    }
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportError::Unreadable { path, detail, .. } => {
                write!(f, "cannot read test results {}: {detail}", path.display())
            }
            ReportError::Malformed { path, detail } => write!(
                f,
                "invalid test results {}: {detail} (expected the RES-15 specforge-report.json shape)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ReportError {}

/// The project's recorded test results (`specforge-report.json` at `root`,
/// written by `specforge collect`). No report means no recorded tests
/// (`Ok(None)`); a report that is there but unreadable or malformed is an
/// error, never read as empty, so coverage cannot silently drop.
pub fn read_report(root: &Path) -> Result<Option<TestReport>, ReportError> {
    let path = root.join(REPORT_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => parse_report(&path, &bytes).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(unreadable(&path, &e)),
    }
}

/// A test report at an explicit path (`--test-results`), which must exist.
pub fn read_report_file(path: &Path) -> Result<TestReport, ReportError> {
    let bytes = std::fs::read(path).map_err(|e| unreadable(path, &e))?;
    parse_report(path, &bytes)
}

fn unreadable(path: &Path, error: &std::io::Error) -> ReportError {
    ReportError::Unreadable {
        path: path.to_path_buf(),
        io: error.kind(),
        detail: error.to_string(),
    }
}

/// The report `path` holds, from its bytes.
fn parse_report(path: &Path, bytes: &[u8]) -> Result<TestReport, ReportError> {
    let raw = std::str::from_utf8(bytes).map_err(|_| ReportError::Malformed {
        path: path.to_path_buf(),
        detail: "stream did not contain valid UTF-8".to_string(),
    })?;
    serde_json::from_str(raw).map_err(|e| ReportError::Malformed {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

/// A project's coverage, computed by the one rule the `coverage` pass
/// applies (ADR 0004, D2-f). Per-entity views and stats read its verdicts
/// and summary; standings are the snapshot's.
///
/// Formal discharge needs the prove pass, which a per-entity view does not
/// run: as `analyze coverage` without `--prove`, a `verify property`
/// obligation is proven only by a passing test. The host grades no kind by
/// risk (ADR 0009, B): its summary has no risk tallies and its findings no
/// A002; the risk grading is `@specforge/testing`'s.
#[derive(Debug, Clone, Default)]
pub struct ProjectCoverage {
    /// The snapshot it was computed from.
    entities: Arc<EntitySnapshot>,
    /// Per entity id, for every entity in the graph.
    pub verdicts: BTreeMap<String, Verdict>,
    /// The summary the `coverage` pass reports for the same inputs, less
    /// what its risk grading adds (the risk tallies and enforcement counts).
    pub summary: Summary,
}

impl ProjectCoverage {
    /// Score the snapshot's entities against their recorded tests (`None`
    /// without a report). Callers read it through a [`RecordedCoverage`],
    /// which computes it once per compile and report content.
    pub(crate) fn compute(entities: &Arc<EntitySnapshot>, report: Option<&TestReport>) -> Self {
        let rule_entities = entities.coverage_entities();
        let results = report.map(TestResults::from);
        let assessment = specforge_coverage::assess(&rule_entities, results.as_ref(), None, None);
        ProjectCoverage {
            entities: Arc::clone(entities),
            verdicts: assessment.verdicts,
            summary: assessment.summary,
        }
    }

    /// The entity's verdict, if the graph has it.
    pub fn verdict(&self, id: &str) -> Option<&Verdict> {
        self.verdicts.get(id)
    }

    /// The entity's status; an entity the graph doesn't have is uncovered.
    pub fn status(&self, id: &str) -> Status {
        self.verdict(id).map_or(Status::Uncovered, Verdict::status)
    }

    /// Whether the entity counts toward coverage (ADR 0004 D2-b) and is not
    /// proven (D2-a): the one definition of "unverified". An entity the
    /// graph doesn't have is not.
    pub fn is_unverified(&self, id: &str) -> bool {
        self.entities.standing(id).is_some_and(Standing::counts)
            && !self.verdict(id).is_some_and(Verdict::is_proven)
    }
}

/// The entity snapshot of a graph, the recorded test report at a root and
/// the coverage computed from both, memoized. Owned by whoever owns the
/// graph it scores: made only by the [`crate::CompiledProject`] that owns it, one-shot or a session's (ADR 0108; the session ([`crate::ProjectSession`]) starts a fresh one on every update and
/// reload), so "once per compile" holds by construction. The memo is bound
/// to its snapshot when it is made: seeded with the one the owner's checks
/// read ([`Self::of`]), or with one taken from the graph and environment it
/// is made over ([`Self::over`]). Nothing asks it for a snapshot with
/// inputs of its own, so it cannot score a graph other than its owner's.
/// Within one, the report is keyed on its path and content, so a rewritten
/// report is read again.
#[derive(Debug)]
pub struct RecordedCoverage {
    entities: Arc<EntitySnapshot>,
    memo: Mutex<Option<Arc<Memo>>>,
}

#[derive(Debug)]
struct Memo {
    path: Option<PathBuf>,
    /// A hash of the report's bytes; `None` without a report.
    content: Option<u64>,
    report: Option<Arc<TestReport>>,
    /// Computed on first use: a report read for its own sake needs none.
    coverage: OnceLock<Arc<ProjectCoverage>>,
}

/// The recorded report at a root, and the coverage computed from it.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub report: Option<Arc<TestReport>>,
    pub coverage: Arc<ProjectCoverage>,
}

/// A copy starts with the coverage already computed, and re-reads the report
/// on its own when the report's bytes change (the memo is keyed on content).
impl Clone for RecordedCoverage {
    fn clone(&self) -> Self {
        let memo = self.memo.lock().unwrap_or_else(|e| e.into_inner()).clone();
        RecordedCoverage {
            entities: Arc::clone(&self.entities),
            memo: Mutex::new(memo),
        }
    }
}

impl RecordedCoverage {
    /// A memo seeded with the snapshot its owner's checks read.
    pub(crate) fn of(entities: Arc<EntitySnapshot>) -> Self {
        RecordedCoverage {
            entities,
            memo: Mutex::default(),
        }
    }

    /// A memo over `graph`, built in `env` (a graph assembled in a test, a
    /// session's graph after an update that skipped the checks): its
    /// snapshot is taken now, with `env`'s registries and spec root.
    pub(crate) fn over(graph: &Graph, env: &Environment) -> Self {
        Self::of(Arc::new(env.entity_snapshot(graph)))
    }

    /// The entity snapshot of the graph this memo scores.
    pub fn entities(&self) -> &Arc<EntitySnapshot> {
        &self.entities
    }

    /// `<root>/specforge-report.json` ([`REPORT_FILE`]): `Ok(None)` without
    /// a root or a file, an error when it is there but unusable. Read again
    /// only when its bytes changed since the last call; an error is never
    /// memoized.
    pub fn report(&self, root: Option<&Path>) -> Result<Option<Arc<TestReport>>, ReportError> {
        Ok(self.memo(root)?.report.clone())
    }

    /// The recorded report at `root` and the coverage of the memo's entity
    /// snapshot against it, computed once per report content.
    pub fn at(&self, root: Option<&Path>) -> Result<Recorded, ReportError> {
        let memo = self.memo(root)?;
        let coverage = memo
            .coverage
            .get_or_init(|| {
                Arc::new(ProjectCoverage::compute(
                    &self.entities,
                    memo.report.as_deref(),
                ))
            })
            .clone();
        Ok(Recorded {
            report: memo.report.clone(),
            coverage,
        })
    }

    /// The memo for the report at `root` as it is on disk now.
    fn memo(&self, root: Option<&Path>) -> Result<Arc<Memo>, ReportError> {
        let path = root.map(|root| root.join(REPORT_FILE));
        let bytes = match &path {
            None => None,
            Some(path) => match std::fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(unreadable(path, &e)),
            },
        };
        let content = bytes.as_deref().map(|bytes| {
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            hasher.finish()
        });
        let mut slot = self.memo.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(memo) = slot.as_ref()
            && memo.path == path
            && memo.content == content
        {
            return Ok(Arc::clone(memo));
        }
        let report = match (&path, &bytes) {
            (Some(path), Some(bytes)) => Some(Arc::new(parse_report(path, bytes)?)),
            _ => None,
        };
        let memo = Arc::new(Memo {
            path,
            content,
            report,
            coverage: OnceLock::new(),
        });
        *slot = Some(Arc::clone(&memo));
        Ok(memo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_protocol_types::{
        EntityKindDescriptor, ExtensionDeclaration, ValidationRuleDescriptor, ValidationSeverity,
    };
    use specforge_registry::{RegistryBuild, build_registries};
    use specforge_test_macros::test as specforge_test;

    fn graph_of(source: &str) -> Graph {
        let (graph, _) =
            specforge_graph::build_graph(&[specforge_parser::parse(source, "test.spec")]);
        graph
    }

    /// The registry build of one extension declaring the testable `behavior`
    /// kind and one W004 rule on it.
    fn w004_registries() -> RegistryBuild {
        let mut declaration = ExtensionDeclaration {
            entities: vec![EntityKindDescriptor {
                name: "behavior".into(),
                keyword: Some("behavior".into()),
                testable: true,
                supports_verify: true,
                ..Default::default()
            }],
            validation_rules: vec![ValidationRuleDescriptor {
                code: "W004".into(),
                severity: ValidationSeverity::Warning,
                message_template: "{kind} '{id}' is testable but declares no verify obligations"
                    .into(),
                check: "no_verify_statements".into(),
                target_kind: Some("behavior".into()),
                field: Some("verify".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        declaration.handshake.name = "@specforge/testing".into();
        declaration.handshake.version = "1.0.0".into();
        let build = build_registries(vec![declaration]);
        assert!(
            build.diagnostics().next().is_none(),
            "{:?}",
            build.diagnostics().collect::<Vec<_>>()
        );
        build
    }

    /// A project whose `behavior` kind is testable and must declare
    /// obligations, with `login` (one obligation) and `logout` (none).
    struct Scored {
        graph: Graph,
        registries: RegistryBuild,
    }

    impl Scored {
        fn new() -> Self {
            let registries = w004_registries();
            Scored {
                graph: graph_of(
                    "behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\n\
                     behavior logout \"Logout\" {\n}\n",
                ),
                registries,
            }
        }

        /// The snapshot its compile would seed the memo with.
        fn entities(&self) -> Arc<EntitySnapshot> {
            Arc::new(EntitySnapshot::of(
                &self.graph,
                &self.registries,
                Path::new(""),
            ))
        }
    }

    fn report(status: &str) -> String {
        format!(
            r#"{{"runner": "r", "results": {{"login": {{"tests": [
                {{"name": "t", "status": "{status}", "verify": "logs in"}}]}}}}}}"#
        )
    }

    #[specforge_test(
        behavior = "read_views_over_the_project_view",
        verify = "coverage is computed once per compile and report content, and again after the report changes"
    )]
    fn recorded_coverage_is_memoized_per_report_content() {
        let dir = tempfile::tempdir().unwrap();
        let project = Scored::new();
        let entities = project.entities();
        let recorded = RecordedCoverage::of(Arc::clone(&entities));
        let at = || recorded.at(Some(dir.path()));

        // No report: nothing recorded, nothing proven.
        let none = at().unwrap();
        assert!(none.report.is_none());
        // The coverage scores the seeded snapshot, not one of its own.
        assert!(Arc::ptr_eq(&none.coverage.entities, &entities));
        assert!(!none.coverage.verdict("login").unwrap().is_proven());

        std::fs::write(dir.path().join(REPORT_FILE), report("passed")).unwrap();
        let first = at().unwrap();
        let second = at().unwrap();
        assert!(Arc::ptr_eq(&first.coverage, &second.coverage));
        assert!(Arc::ptr_eq(
            first.report.as_ref().unwrap(),
            second.report.as_ref().unwrap()
        ));
        assert!(first.coverage.verdict("login").unwrap().is_proven());
        assert!(Arc::ptr_eq(
            &recorded.report(Some(dir.path())).unwrap().unwrap(),
            first.report.as_ref().unwrap()
        ));

        // Other bytes, read again at once (no mtime to wait for).
        std::fs::write(dir.path().join(REPORT_FILE), report("failed")).unwrap();
        let rewritten = at().unwrap();
        assert!(!Arc::ptr_eq(&first.coverage, &rewritten.coverage));
        assert_eq!(rewritten.coverage.verdict("login").unwrap().failing, 1);

        // A malformed report is an error every time: never memoized.
        std::fs::write(dir.path().join(REPORT_FILE), "{not json").unwrap();
        assert!(matches!(at(), Err(ReportError::Malformed { .. })));
        assert!(matches!(at(), Err(ReportError::Malformed { .. })));

        // Without a root there is no report to read.
        let rootless = recorded.at(None).unwrap();
        assert!(rootless.report.is_none());

        // A memo is made with its snapshot and hands that one out: its
        // coverage scores it, not one taken later from other inputs.
        let env = Environment::assemble(
            specforge_common::parse_project_config(
                &dir.path().join("specforge.json"),
                r#"{"spec_root": "spec"}"#,
            ),
            specforge_installed::Installed::unread(dir.path()),
            crate::ExtensionLoad::NoRuntime,
        );
        let over = RecordedCoverage::over(&project.graph, &env);
        assert_eq!(over.entities().spec_root(), env.spec_root());
        assert!(Arc::ptr_eq(
            &over.at(None).unwrap().coverage.entities,
            over.entities()
        ));
    }

    #[specforge_test(
        behavior = "ingest_collector_report",
        verify = "the recorded test report speaks the collect vocabulary, and a status outside it is a report that cannot be used"
    )]
    fn the_report_speaks_the_collect_vocabulary() {
        let project = Scored::new();
        let verdict_of = |status: &str| {
            let tests: TestReport = serde_json::from_str(&report(status)).unwrap();
            let coverage = ProjectCoverage::compute(&project.entities(), Some(&tests));
            coverage.verdict("login").unwrap().clone()
        };
        assert!(verdict_of("passed").is_proven());
        assert_eq!(verdict_of("failed").failing, 1);
        let skipped = verdict_of("skipped");
        assert_eq!((skipped.tests, skipped.failing), (1, 0));
        assert!(!skipped.is_proven());

        let path = Path::new("specforge-report.json");
        let Err(ReportError::Malformed { detail, .. }) =
            parse_report(path, report("pass").as_bytes())
        else {
            panic!("a report saying `pass` must be malformed");
        };
        assert!(detail.contains("unknown variant `pass`"), "{detail}");
        let nameless =
            r#"{"runner": "r", "results": {"login": {"tests": [{"status": "passed"}]}}}"#;
        let Err(ReportError::Malformed { detail, .. }) = parse_report(path, nameless.as_bytes())
        else {
            panic!("a test without a name must be malformed");
        };
        assert!(detail.contains("missing field `name`"), "{detail}");
    }

    #[specforge_test(
        behavior = "read_views_over_the_project_view",
        verify = "an entity is unverified when it counts toward coverage and is not proven"
    )]
    fn every_entity_has_a_standing_and_unverified_reads_it() {
        let project = Scored::new();
        let tests: TestReport = serde_json::from_str(&report("passed")).unwrap();
        let coverage = ProjectCoverage::compute(&project.entities(), Some(&tests));
        let login = project.entities().standing("login").unwrap().clone();
        assert!(login.testable && login.counts() && !login.exempt());
        assert!(!coverage.is_unverified("login"), "proven");
        assert!(
            coverage.is_unverified("logout"),
            "counts and owes an obligation"
        );
        assert!(!coverage.is_unverified("nobody"));
    }
}
