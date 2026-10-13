//! Every wasm component this repository commits, where each is built from, and what an SDK
//! blob answers on the wire. Test support (feature `testing`): the declaration snapshots and
//! their tests, `xtask build-blobs` and `xtask revendor` read it, and every test that loads
//! a fixture blob names it here. No production code reads it.
//!
//! A committed blob lives at `<crate_dir>/wasm/<artifact>`, beside the `inputs.json` that
//! `build-blobs --install` records for it. `git ls-files '*.wasm'` is exactly [`COMMITTED`]
//! (`xtask`'s `every_committed_wasm_is_listed`).

use crate::builtins::BUILTINS;
use specforge_protocol_types::{
    DeclaredCategory, DescribeRequest, HandshakeRequest, PROTOCOL_VERSION,
};
use specforge_wasm::runtime::{WasmCallResult, WasmRuntime};
use std::path::PathBuf;

/// What a committed blob is: it decides what the blob must answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guest {
    /// Built with `specforge-extension-sdk`. Its handshake says the host's `PROTOCOL_VERSION`.
    /// Its handshake and every describe answer are pinned under [`snapshots`]`/<snapshot_dir>/`,
    /// and they round-trip through `ExtensionDeclaration` byte for byte. The exception is
    /// `legacy_keys`: (category, key) pairs the blob declares on purpose although the protocol
    /// no longer defines them. Its declaration must report exactly those as unknown keys, and a
    /// category holding one is not round-tripped.
    Sdk {
        legacy_keys: &'static [(&'static str, &'static str)],
    },
    /// A `specforge:bridge` component written without the SDK, for the component runtime's own
    /// tests. The extension load never reads it. It answers `__handshake` with `canned_protocol`
    /// whatever the host speaks, it has no declaration, and nothing pins its answers.
    Bridge { canned_protocol: &'static str },
}

/// One committed blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommittedBlob {
    /// The name it is loaded under: an SDK blob's declared name.
    pub name: &'static str,
    /// Its crate, from the repository root: `extensions/software`, `fixtures/greet-extension`.
    pub crate_dir: &'static str,
    /// The file `cargo build --release --target wasm32-wasip2` produces for it.
    pub artifact: &'static str,
    pub guest: Guest,
}

impl CommittedBlob {
    /// `<crate_dir>/wasm/<artifact>`, from the repository root: where git holds it.
    pub fn repo_path(&self) -> String {
        format!("{}/wasm/{}", self.crate_dir, self.artifact)
    }

    /// [`Self::repo_path`] under [`repo_root`].
    pub fn path(&self) -> PathBuf {
        repo_root().join(self.repo_path())
    }

    /// The committed bytes. Panics naming the file when they cannot be read.
    pub fn read(&self) -> Vec<u8> {
        let path = self.path();
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The directory its wire answers are pinned in, under [`snapshots`]: its name's last
    /// segment (`software`, `greet`, `probe`).
    pub fn snapshot_dir(&self) -> &'static str {
        self.name.rsplit('/').next().unwrap_or(self.name)
    }

    /// Whether it is built with the SDK.
    pub const fn is_sdk(&self) -> bool {
        matches!(self.guest, Guest::Sdk { .. })
    }
}

/// `@sdk/greet`, the SDK's worked example (`docs/extension-sdk.md`).
pub const GREET: CommittedBlob = CommittedBlob {
    name: "@sdk/greet",
    crate_dir: "fixtures/greet-extension",
    artifact: "greet.wasm",
    guest: Guest::Sdk { legacy_keys: &[] },
};

/// `@sdk/docref`: a kind whose `docs` field names files; the session, watch and LSP tests read it.
pub const DOCREF: CommittedBlob = CommittedBlob {
    name: "@sdk/docref",
    crate_dir: "fixtures/docref-extension",
    artifact: "docref.wasm",
    guest: Guest::Sdk { legacy_keys: &[] },
};

/// `@test/probe`: surface exports that try every capability. It describes its surfaces as a
/// guest built before ADR 0037 did, each with a `sandbox` override (W153 at load).
pub const PROBE: CommittedBlob = CommittedBlob {
    name: "@test/probe",
    crate_dir: "fixtures/sandbox-probe",
    artifact: "sandbox_probe.wasm",
    guest: Guest::Sdk {
        legacy_keys: &[("surfaces", "sandbox")],
    },
};

/// `@test/component`: the minimal bridge component the runtime tests load (handshake, echo).
pub const BRIDGE: CommittedBlob = CommittedBlob {
    name: "@test/component",
    crate_dir: "fixtures/test-component",
    artifact: "specforge_test_component.wasm",
    guest: Guest::Bridge {
        canned_protocol: "1.0.0",
    },
};

/// The fixtures, in the order [`COMMITTED`] lists them after the builtins.
pub const FIXTURES: [CommittedBlob; 4] = [GREET, DOCREF, PROBE, BRIDGE];

/// Every committed blob: the builtins in [`BUILTINS`] order, then [`FIXTURES`].
pub const COMMITTED: &[CommittedBlob] = &{
    let mut out = [GREET; BUILTINS.len() + FIXTURES.len()];
    let mut i = 0;
    while i < BUILTINS.len() {
        out[i] = CommittedBlob {
            name: BUILTINS[i].name,
            crate_dir: BUILTINS[i].crate_dir,
            artifact: BUILTINS[i].artifact,
            guest: Guest::Sdk { legacy_keys: &[] },
        };
        i += 1;
    }
    let mut j = 0;
    while j < FIXTURES.len() {
        out[BUILTINS.len() + j] = FIXTURES[j];
        j += 1;
    }
    out
};

/// The repository root: this crate's manifest directory, two levels up.
pub fn repo_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the crate lives in <root>/crates/")
        .to_path_buf()
}

/// Where the SDK blobs' wire answers are pinned: `crates/specforge-component/tests/declarations/`.
pub fn snapshots() -> PathBuf {
    repo_root().join("crates/specforge-component/tests/declarations")
}

/// One wire answer: the file it is pinned in (`handshake.json`, `describe_<category>.json`)
/// and its exact bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireAnswer {
    pub file: String,
    pub bytes: Vec<u8>,
}

/// Every wire answer of `blob`, which `runtime` has loaded under `blob.name`: its handshake to
/// a `HandshakeRequest` carrying the host's `PROTOCOL_VERSION`, then its describe answer to a
/// `DescribeRequest` for each of `DeclaredCategory::ALL`, in that order. The exports are
/// called raw because the bytes are what is pinned (ADR 0013). The error names the export,
/// the request and the trap.
pub fn wire_answers(
    runtime: &dyn WasmRuntime,
    blob: &CommittedBlob,
) -> Result<Vec<WireAnswer>, String> {
    let handshake = serde_json::to_vec(&HandshakeRequest {
        host_version: PROTOCOL_VERSION.to_string(),
    })
    .expect("requests serialize");
    let mut answers = vec![WireAnswer {
        file: "handshake.json".to_string(),
        bytes: call(runtime, blob, "__handshake", &handshake)?,
    }];
    for category in DeclaredCategory::ALL {
        let request = serde_json::to_vec(&DescribeRequest {
            category: category.name().to_string(),
        })
        .expect("requests serialize");
        answers.push(WireAnswer {
            file: format!("describe_{}.json", category.name()),
            bytes: call(runtime, blob, "__describe", &request)?,
        });
    }
    Ok(answers)
}

fn call(
    runtime: &dyn WasmRuntime,
    blob: &CommittedBlob,
    export: &str,
    request: &[u8],
) -> Result<Vec<u8>, String> {
    match runtime.call_export(blob.name, export, request) {
        WasmCallResult::Ok(bytes) => Ok(bytes),
        WasmCallResult::Trap(trap) => Err(format!(
            "{} {export} {}: {}: {}",
            blob.name,
            String::from_utf8_lossy(request),
            trap.kind,
            trap.message
        )),
    }
}
