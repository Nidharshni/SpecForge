//! `collect` end to end in process (ADR 0125): runners declared through the SDK, an in-process runtime
//! answering each export, and a `ScriptedMachine` standing for the machine. Nothing here spawns a
//! process, reads the environment or the wall clock.

mod order;
mod refusals;
mod runs;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_ops::analysis::collect::{
    CollectField, Collector, CommandApproval, Mode, Outcome, Request, collectors,
};
use specforge_ops::asking::Asking;
use specforge_ops::testing::{ScriptedMachine, Viewed};
use specforge_ops::{Caller, Containment, Refusal};
use specforge_protocol_types::{
    CollectEntityResult, CollectOutput, ExtensionDeclaration, TestResult, TestStatus,
};
use specforge_wasm::testing::{InProcessRuntime, RecordedCall};
use specforge_wasm::{WasmCallResult, WasmTrapInfo};

/// What a runner's export answers.
enum Answer {
    Output(CollectOutput),
    Trap,
}

/// One runner extension: a collector `name` of extension `ext`.
pub struct Runner {
    ext: String,
    name: String,
    detect: Vec<String>,
    run: Vec<String>,
    report: String,
    capture: Option<String>,
    answer: Answer,
}

impl Runner {
    /// Collector `name` of `@acme/<name>`: detected by `<name>.cfg`, run as `<name> --out={report}`, its
    /// report `out/<name>.json`, answering nothing.
    pub fn new(name: &str) -> Self {
        Runner {
            ext: format!("@acme/{name}"),
            name: name.to_string(),
            detect: vec![format!("{name}.cfg")],
            run: vec![name.to_string(), "--out={report}".to_string()],
            report: format!("out/{name}.json"),
            capture: None,
            answer: Answer::Output(CollectOutput::default()),
        }
    }

    pub fn run(mut self, argv: &[&str]) -> Self {
        self.run = argv.iter().map(|a| a.to_string()).collect();
        self
    }

    pub fn report(mut self, report: &str) -> Self {
        self.report = report.to_string();
        self
    }

    pub fn capturing_stdout(self) -> Self {
        self.capturing("stdout")
    }

    /// Declares `capture` raw, whatever it is.
    pub fn capturing(mut self, capture: &str) -> Self {
        self.capture = Some(capture.to_string());
        self
    }

    /// Its export answers that `entity`'s test `test` passed.
    pub fn proving(mut self, entity: &str, test: &str) -> Self {
        let Answer::Output(output) = &mut self.answer else {
            panic!("a trapping runner proves nothing");
        };
        output.entity_results.push(CollectEntityResult {
            entity_id: entity.to_string(),
            test_results: vec![TestResult {
                name: test.to_string(),
                status: TestStatus::Passed,
                verify: None,
                duration_ms: None,
            }],
        });
        self
    }

    /// Its export traps.
    pub fn trapping(mut self) -> Self {
        self.answer = Answer::Trap;
        self
    }

    fn export(&self) -> String {
        format!("collect__{}", self.name)
    }

    fn declaration(&self) -> ExtensionDeclaration {
        let detect: Vec<&str> = self.detect.iter().map(String::as_str).collect();
        let run: Vec<&str> = self.run.iter().map(String::as_str).collect();
        let mut builder = ContributionsBuilder::new(ExtensionMeta::new(&self.ext, "1.0.0"));
        builder.collector(&self.name, |c| {
            c.detect_files(&detect)
                .run(&run)
                .collect(|_| Ok(CollectOutput::default()));
        });
        let mut declaration = builder.declaration();
        // The report and the capture as declared, whatever they are.
        declaration.collectors[0].report = Some(self.report.clone());
        declaration.collectors[0].capture = self.capture.clone();
        declaration
    }
}

/// A project with behaviors `a` and `b` and the runners' extensions loaded.
pub struct Fixture {
    dir: tempfile::TempDir,
    project: specforge_project::testing::Assembled,
    collectors: Vec<Collector>,
    pub runtime: Arc<InProcessRuntime>,
}

impl Fixture {
    pub fn new(runners: Vec<Runner>) -> Self {
        let mut runtime = InProcessRuntime::new();
        for runner in &runners {
            let result = match &runner.answer {
                Answer::Output(output) => {
                    WasmCallResult::Ok(serde_json::to_vec(output).expect("output serializes"))
                }
                Answer::Trap => WasmCallResult::Trap(WasmTrapInfo {
                    kind: "trap".to_string(),
                    message: "the export trapped".to_string(),
                    export_name: runner.export(),
                }),
            };
            runtime = runtime.answer_raw(&runner.ext, &runner.export(), result);
        }
        let runtime = Arc::new(runtime);
        let declarations: Vec<ExtensionDeclaration> =
            runners.iter().map(Runner::declaration).collect();
        let collectors = collectors(&declarations);
        let dir = tempfile::tempdir().unwrap();
        let config = r#"{"name": "demo"}"#;
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
        let shared: specforge_project::SharedRuntime = Arc::clone(&runtime) as _;
        let project = specforge_project::testing::Assembled::assembling_at(dir.path(), |root| {
            specforge_project::Environment::assemble(
                specforge_common::parse_project_config(&root.join("specforge.json"), config),
                specforge_installed::Installed::unread(root),
                specforge_project::ExtensionLoad::In {
                    runtime: shared,
                    loaded: specforge_project::Loaded {
                        enabled: Vec::new(),
                        declarations,
                    },
                },
            )
        })
        .source("behavior a \"A\" {\n}\nbehavior b \"B\" {\n}\n");
        Fixture {
            dir,
            project,
            collectors,
            runtime,
        }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Write `contents` at `path` under the root.
    pub fn write(&self, path: &str, contents: &str) {
        let at = self.root().join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, contents).unwrap();
    }

    pub fn exists(&self, path: &str) -> bool {
        self.root().join(path).exists()
    }

    pub fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path)).unwrap()
    }

    /// What `specforge-report.json` holds, when it exists.
    pub fn report(&self) -> Option<serde_json::Value> {
        let raw = std::fs::read_to_string(self.root().join("specforge-report.json")).ok()?;
        Some(serde_json::from_str(&raw).unwrap())
    }

    /// The approval runner `name`'s command needs here.
    pub fn approval(&self, name: &str) -> CommandApproval {
        let collector = self.collectors.iter().find(|c| c.name == name).unwrap();
        collector.approval(self.root())
    }

    /// Where runner `name`'s report is, as the project spells it.
    pub fn report_of(&self, name: &str) -> PathBuf {
        let collector = self.collectors.iter().find(|c| c.name == name).unwrap();
        self.root().join(&collector.report)
    }

    /// Collect in `mode` with `runner` named (or detected), reaching `containment`, asking `asking`.
    pub fn collect_with(
        &self,
        runner: Option<&str>,
        mode: Mode<'_>,
        containment: &Containment,
        asking: Asking<'_>,
        machine: &ScriptedMachine,
    ) -> Result<Outcome, Refusal<CollectField>> {
        let view = self.project.view();
        specforge_ops::analysis::collect::collect(
            &view,
            Request {
                runner,
                mode,
                announce: &mut |_, _| {},
                caller: Caller::new(containment, asking),
            },
            machine,
        )
    }

    /// Run the detected runners, reaching every path.
    pub fn run(
        &self,
        asking: Asking<'_>,
        machine: &ScriptedMachine,
    ) -> Result<Outcome, Refusal<CollectField>> {
        self.collect_with(
            None,
            Mode::Run(specforge_ops::analysis::collect::RunnerOutput::Discard),
            &Containment::unbounded(),
            asking,
            machine,
        )
    }

    /// Read the reports already there, reaching every path.
    pub fn no_run(
        &self,
        runner: Option<&str>,
        machine: &ScriptedMachine,
    ) -> Result<Outcome, Refusal<CollectField>> {
        self.collect_with(
            runner,
            Mode::NoRun,
            &Containment::unbounded(),
            Asking::Nobody,
            machine,
        )
    }

    /// What the exports received, in order.
    pub fn dispatched(&self) -> Vec<RecordedCall> {
        self.runtime.calls()
    }
}

/// The paths of the report files a dispatch received.
pub fn paths(call: &RecordedCall) -> Vec<String> {
    call.input["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap().to_string())
        .collect()
}
