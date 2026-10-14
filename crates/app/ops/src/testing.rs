//! What tests serve in process as candidates: `@sdk/greet` (the fixture's
//! own declarations, `fixtures/greet-extension/src/contributions.rs`) under
//! bytes a test chooses, and builtin look-alikes under a builtin's real
//! embedded bytes (`InProcessRuntime::binary` serves whatever name the
//! candidate is loaded under).
//!
//! It also holds `ScriptedSolver`, the SMT solver tests script (ADR 0079).
//!
//! Behind the `testing` feature; no production code reads it.
#![allow(
    clippy::unwrap_used,
    reason = "test support behind the `testing` feature: a failed unwrap fails the calling test"
)]

use std::collections::VecDeque;
use std::sync::Mutex;

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_protocol_types::PeerDependency;
use specforge_wasm::testing::InProcessRuntime;

use crate::solver::{Availability, SolveFailure, Solver, Z3};

#[path = "../../../../fixtures/greet-extension/src/contributions.rs"]
mod greet;

/// The bytes the in-process runtime serves as `@sdk/greet` 0.1.0.
pub const GREET: &[u8] = b"\0asm in-process @sdk/greet 0.1.0";
/// Other bytes that also serve `@sdk/greet` 0.1.0: a reinstall, a changed
/// binary.
pub const GREET_VARIANT: &[u8] = b"\0asm in-process @sdk/greet 0.1.0 variant";
/// The bytes the in-process runtime serves as `@test/probe` 0.1.0.
pub const PROBE: &[u8] = b"\0asm in-process @test/probe 0.1.0";

/// What `@sdk/greet` declares: the fixture's builder.
pub fn greet() -> ContributionsBuilder {
    greet::build()
}

/// A local file declaring `@probe/needs-software` 1.0.0, requiring `@specforge/software` ^9.0.
pub const NEEDS_SOFTWARE_9: &[u8] = b"\0asm in-process @probe/needs-software 1.0.0 software ^9";
/// The same, requiring `@specforge/software` ^1.0.
pub const NEEDS_SOFTWARE_1: &[u8] = b"\0asm in-process @probe/needs-software 1.0.0 software ^1";
/// `@probe/base` 2.0.0, no peers.
pub const BASE_2: &[u8] = b"\0asm in-process @probe/base 2.0.0";
/// `@probe/base` 1.2.0, no peers.
pub const BASE_1: &[u8] = b"\0asm in-process @probe/base 1.2.0";
/// `@probe/dep` 1.0.0, requiring `@probe/base` ^1.0.
pub const DEP_1: &[u8] = b"\0asm in-process @probe/dep 1.0.0";

/// An extension named `name` at `version` whose peers are `peers` (name,
/// requirement, optional).
pub fn declaring(
    name: &'static str,
    version: &'static str,
    peers: &'static [(&'static str, &'static str, bool)],
) -> impl Fn() -> ContributionsBuilder + Send + Sync + 'static {
    move || {
        let mut meta = ExtensionMeta::new(name, version);
        meta.peer_dependencies = peers
            .iter()
            .map(|(peer, requirement, optional)| PeerDependency {
                name: (*peer).to_string(),
                version: (*requirement).to_string(),
                optional: *optional,
            })
            .collect();
        ContributionsBuilder::new(meta)
    }
}

/// An in-process runtime serving `GREET` and `GREET_VARIANT` as `@sdk/greet`
/// and `PROBE` as `@test/probe`.
pub fn candidates() -> InProcessRuntime {
    InProcessRuntime::new()
        .binary(GREET, greet)
        .binary(GREET_VARIANT, greet)
        .binary(PROBE, declaring("@test/probe", "0.1.0", &[]))
}

/// `runtime`, also serving the embedded bytes of the builtin `name` as
/// `build` declares it.
pub fn serving_builtin(
    runtime: InProcessRuntime,
    name: &str,
    build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static,
) -> InProcessRuntime {
    runtime.binary(
        specforge_project::builtins()
            .get(name)
            .expect("a builtin extension"),
        build,
    )
}

/// [`candidates`], also serving the probes above and the builtins `@specforge/software` 1.0.0 and
/// `@specforge/testing` 1.0.0 (no peers) from their embedded bytes.
pub fn probes() -> InProcessRuntime {
    let runtime = candidates()
        .binary(
            NEEDS_SOFTWARE_9,
            declaring(
                "@probe/needs-software",
                "1.0.0",
                &[("@specforge/software", "^9.0", false)],
            ),
        )
        .binary(
            NEEDS_SOFTWARE_1,
            declaring(
                "@probe/needs-software",
                "1.0.0",
                &[("@specforge/software", "^1.0", false)],
            ),
        )
        .binary(BASE_2, declaring("@probe/base", "2.0.0", &[]))
        .binary(BASE_1, declaring("@probe/base", "1.2.0", &[]))
        .binary(
            DEP_1,
            declaring("@probe/dep", "1.0.0", &[("@probe/base", "^1.0", false)]),
        );
    let runtime = serving_builtin(
        runtime,
        "@specforge/software",
        declaring("@specforge/software", "1.0.0", &[]),
    );
    serving_builtin(
        runtime,
        "@specforge/testing",
        declaring("@specforge/testing", "1.0.0", &[]),
    )
}

/// A solver that answers what a test scripts: its availability, then each
/// solve's stdout (or failure) in order. It keeps every script it was asked.
pub struct ScriptedSolver {
    availability: Availability,
    answers: Mutex<VecDeque<Result<String, SolveFailure>>>,
    scripts: Mutex<Vec<String>>,
}

impl ScriptedSolver {
    fn with(availability: Availability, answers: VecDeque<Result<String, SolveFailure>>) -> Self {
        Self {
            availability,
            answers: Mutex::new(answers),
            scripts: Mutex::new(Vec::new()),
        }
    }

    /// z3 runs (`Z3 version 4.13.0 - scripted`) and is asked nothing.
    pub fn ready() -> Self {
        Self::answering([])
    }

    /// z3 runs and answers `answers`, one per solve, in order.
    pub fn answering(answers: impl IntoIterator<Item = Result<String, SolveFailure>>) -> Self {
        Self::with(
            Availability::Ready {
                version: "Z3 version 4.13.0 - scripted".into(),
            },
            answers.into_iter().collect(),
        )
    }

    /// z3 is not on PATH: the W098 the z3 adapter gives for it.
    pub fn not_found() -> Self {
        Self::with(
            Availability::Unavailable(Z3::not_on_path()),
            VecDeque::new(),
        )
    }

    /// The scripts it was asked, in order.
    pub fn scripts(&self) -> Vec<String> {
        self.scripts.lock().unwrap().clone()
    }
}

impl Solver for ScriptedSolver {
    fn availability(&self) -> Availability {
        self.availability.clone()
    }

    fn solve(&self, script: &str) -> Result<String, SolveFailure> {
        self.scripts.lock().unwrap().push(script.to_string());
        self.answers.lock().unwrap().pop_front().unwrap_or_else(|| {
            panic!("the scripted solver was asked more than it was scripted to answer:\n{script}")
        })
    }
}
