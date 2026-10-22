//! The host's coverage: how the recorded test report is read and what it
//! proves of the entity snapshot's entities (ADR 0019, ADR 0004 D2-f).
//!
//! Every surface that asks what an entity's recorded tests prove (stats,
//! the coverage view, inspect's coverage, an entity's coverage status
//! (`statuses`), query with coverage, the review,
//! the trace prompt's unverified entities, an extension command's
//! evidence) reads it here, so they cannot disagree. Who is testable and
//! what an entity declares are the snapshot's standing
//! (`ProjectView::entities`), which reads no report.

use crate::Environment;
use serde::Deserialize;
use specforge_graph::GraphBuild;
use specforge_protocol_types::{EntityTests, TestResult, TestResults};
use specforge_registry::{Disk, EntitySnapshot, Standing};
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

    /// `path` could not be read: `error`, kept by kind.
    pub fn unreadable(path: &Path, error: &std::io::Error) -> Self {
        ReportError::Unreadable {
            path: path.to_path_buf(),
            io: error.kind(),
            detail: error.to_string(),
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

impl TestReport {
    /// The report `bytes` hold, named `path` in its errors: not UTF-8 or not the RES-15 shape
    /// is [`ReportError::Malformed`]. The one parser of `specforge-report.json`.
    pub fn parse(path: &Path, bytes: &[u8]) -> Result<TestReport, ReportError> {
        let raw = std::str::from_utf8(bytes).map_err(|_| ReportError::Malformed {
            path: path.to_path_buf(),
            detail: "stream did not contain valid UTF-8".to_string(),
        })?;
        serde_json::from_str(raw).map_err(|e| ReportError::Malformed {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })
    }
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

    /// Each entity's coverage status (ADR 0216), by id: its standing's status
    /// for its verdict. What a query's export nodes carry as `coverage_status`.
    pub fn statuses(&self) -> BTreeMap<String, Status> {
        self.entities
            .iter()
            .filter_map(|(record, standing)| {
                Some((
                    record.id.clone(),
                    standing.status(self.verdict(&record.id)?),
                ))
            })
            .collect()
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
pub(crate) struct RecordedCoverage {
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

/// The recorded report, and the coverage computed from it.
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
    pub(crate) fn over(graph: &GraphBuild, env: &Environment) -> Self {
        Self::of(Arc::new(env.entity_snapshot(graph)))
    }

    /// The entity snapshot of the graph this memo scores.
    pub(crate) fn entities(&self) -> &Arc<EntitySnapshot> {
        &self.entities
    }

    /// The recorded report `file` names, read through `disk`: `Ok(None)` without a file name or
    /// when the disk has no such file, an error when it is there but unusable. Read again only
    /// when its bytes changed since the last call; an error is never memoized.
    pub(crate) fn report(
        &self,
        file: Option<&Path>,
        disk: &dyn Disk,
    ) -> Result<Option<Arc<TestReport>>, ReportError> {
        Ok(self.memo(file, disk)?.report.clone())
    }

    /// The recorded report `file` names and the coverage of the memo's entity snapshot against
    /// it, computed once per report content.
    pub(crate) fn at(&self, file: Option<&Path>, disk: &dyn Disk) -> Result<Recorded, ReportError> {
        let memo = self.memo(file, disk)?;
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

    /// The memo for the report `file` names as `disk` has it now: one read.
    fn memo(&self, file: Option<&Path>, disk: &dyn Disk) -> Result<Arc<Memo>, ReportError> {
        let path = file.map(Path::to_path_buf);
        let bytes = match &path {
            None => None,
            Some(path) => match disk.read(path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(ReportError::unreadable(path, &e)),
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
            (Some(path), Some(bytes)) => Some(Arc::new(TestReport::parse(path, bytes)?)),
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
    use specforge_registry::{MemoryDisk, RegistryBuild, build_registries};
    use specforge_test_macros::test as specforge_test;

    /// `source` built with the graph inputs of `registries`, as the session builds it.
    fn graph_of(source: &str, registries: &RegistryBuild) -> GraphBuild {
        registries.graph_of(
            [specforge_parser::parse(source, "test.spec")],
            std::collections::HashSet::new(),
        )
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
        graph: GraphBuild,
        registries: RegistryBuild,
    }

    impl Scored {
        fn new() -> Self {
            let registries = w004_registries();
            Scored {
                graph: graph_of(
                    "behavior login \"Login\" {\n  verify unit \"logs in\"\n}\n\n\
                     behavior logout \"Logout\" {\n}\n",
                    &registries,
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
        const FILE: &str = "/p/specforge-report.json";
        let project = Scored::new();
        let entities = project.entities();
        let recorded = RecordedCoverage::of(Arc::clone(&entities));
        let with = |text: &str| MemoryDisk::new().with_file(FILE, text.as_bytes());
        let at = |disk: &MemoryDisk| recorded.at(Some(Path::new(FILE)), disk);

        // No report: nothing recorded, nothing proven.
        let none = at(&MemoryDisk::new()).unwrap();
        assert!(none.report.is_none());
        // The coverage scores the seeded snapshot, not one of its own.
        assert!(Arc::ptr_eq(&none.coverage.entities, &entities));
        assert!(!none.coverage.verdict("login").unwrap().is_proven());

        let disk = with(&report("passed"));
        let first = at(&disk).unwrap();
        let second = at(&disk).unwrap();
        // One read of the file per call.
        assert_eq!(disk.asked().len(), 2);
        assert!(Arc::ptr_eq(&first.coverage, &second.coverage));
        assert!(Arc::ptr_eq(
            first.report.as_ref().unwrap(),
            second.report.as_ref().unwrap()
        ));
        assert!(first.coverage.verdict("login").unwrap().is_proven());
        assert!(Arc::ptr_eq(
            &recorded
                .report(Some(Path::new(FILE)), &disk)
                .unwrap()
                .unwrap(),
            first.report.as_ref().unwrap()
        ));

        // Other bytes, read again at once (no mtime to wait for).
        let rewritten = at(&with(&report("failed"))).unwrap();
        assert!(!Arc::ptr_eq(&first.coverage, &rewritten.coverage));
        assert_eq!(rewritten.coverage.verdict("login").unwrap().failing, 1);

        // A malformed report is an error every time: never memoized.
        let malformed = with("{not json");
        assert!(matches!(at(&malformed), Err(ReportError::Malformed { .. })));
        assert!(matches!(at(&malformed), Err(ReportError::Malformed { .. })));

        // Without a file there is no report to read, and the disk is not asked.
        let unasked = MemoryDisk::new();
        let rootless = recorded.at(None, &unasked).unwrap();
        assert!(rootless.report.is_none());
        assert!(unasked.asked().is_empty());

        // A memo is made with its snapshot and hands that one out: its
        // coverage scores it, not one taken later from other inputs.
        let root = Path::new("/p");
        let env = Environment::assemble(
            specforge_common::parse_project_config(
                &root.join("specforge.json"),
                r#"{"spec_root": "spec"}"#,
            ),
            specforge_installed::Installed::unread(root),
            crate::ExtensionLoad::NoRuntime,
        );
        let over = RecordedCoverage::over(&env.graph_of([]), &env);
        assert_eq!(over.entities().spec_root(), env.spec_root());
        assert!(Arc::ptr_eq(
            &over.at(None, &unasked).unwrap().coverage.entities,
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
            TestReport::parse(path, report("pass").as_bytes())
        else {
            panic!("a report saying `pass` must be malformed");
        };
        assert!(detail.contains("unknown variant `pass`"), "{detail}");
        let nameless =
            r#"{"runner": "r", "results": {"login": {"tests": [{"status": "passed"}]}}}"#;
        let Err(ReportError::Malformed { detail, .. }) =
            TestReport::parse(path, nameless.as_bytes())
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
        assert!(login.testable && login.counts() && !login.counting().exempt());
        assert!(!coverage.is_unverified("login"), "proven");
        assert!(
            coverage.is_unverified("logout"),
            "counts and owes an obligation"
        );
        assert!(!coverage.is_unverified("nobody"));
    }

    #[test]
    fn statuses_are_each_standings_status_for_its_verdict() {
        let project = Scored::new();
        let tests: TestReport = serde_json::from_str(&report("passed")).unwrap();
        let coverage = ProjectCoverage::compute(&project.entities(), Some(&tests));
        let statuses = coverage.statuses();
        assert_eq!(statuses["login"], Status::Covered);
        assert_eq!(statuses["logout"], Status::Uncovered);
        assert_eq!(statuses.len(), coverage.verdicts.len());
    }
}
