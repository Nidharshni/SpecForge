//! Packages published to the real registry server running in process (`LocalRegistry`, ADR 0044): a test
//! describes what a registry holds, and `specforge add` and `update` reach it over HTTP.

use specforge_registry_client::SigningKey;
use specforge_registry_server::testing::LocalRegistry;
use specforge_wasm::WasmRuntime as _;

use super::TestProject;

/// A published package.
#[derive(Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub wasm: Vec<u8>,
    /// `(name, range)` peers its manifest declares.
    pub peers: Vec<(String, String)>,
    /// The publisher key that signed it; unsigned when `None`.
    pub signer: Option<SigningKey>,
}

impl Package {
    pub fn new(name: &str, version: &str, wasm: Vec<u8>) -> Self {
        Package {
            name: name.to_string(),
            version: version.to_string(),
            wasm,
            peers: Vec::new(),
            signer: None,
        }
    }

    pub fn signed_by(mut self, key: &SigningKey) -> Self {
        self.signer = Some(key.clone());
        self
    }

    pub fn with_peer(mut self, name: &str, range: &str) -> Self {
        self.peers.push((name.to_string(), range.to_string()));
        self
    }

    /// The package's manifest: its declaration (ADR 0012). A loadable binary is published with what it
    /// declares, plus any peer `with_peer` adds; other bytes with a declaration of just its name, version
    /// and peers.
    pub fn manifest(&self) -> String {
        let peers: Vec<serde_json::Value> = self
            .peers
            .iter()
            .map(|(name, range)| serde_json::json!({"name": name, "version": range}))
            .collect();
        let mut declaration = match declared(&self.wasm) {
            Some(declaration) => serde_json::to_value(declaration).unwrap(),
            None => serde_json::json!({
                "handshake": {
                    "protocol_version": "1.0.0",
                    "name": self.name,
                    "version": self.version,
                    "peer_dependencies": [],
                    "sandbox_policy": null,
                }
            }),
        };
        declaration["handshake"]["peer_dependencies"]
            .as_array_mut()
            .unwrap()
            .extend(peers);
        declaration.to_string()
    }
}

/// What `wasm` declares, when it is a loadable extension.
fn declared(wasm: &[u8]) -> Option<specforge_protocol_types::ExtensionDeclaration> {
    let runtime = specforge_component::ComponentRuntime::new();
    runtime.load("__served", wasm).ok()?;
    specforge_wasm::protocol::load_declaration(&runtime, "__served")
        .ok()
        .map(|loaded| loaded.declaration)
}

/// A registry holding `packages`, in order.
pub fn serve(packages: Vec<Package>) -> LocalRegistry {
    let registry = LocalRegistry::start();
    for package in packages {
        let metadata = specforge_registry_client::testing::package(
            &package.name,
            &package.version,
            &package.wasm,
            &package.manifest(),
            package.signer.as_ref(),
        );
        registry.store(&metadata, &package.wasm);
    }
    registry
}

/// The greet extension, vendored.
pub(crate) fn greet_wasm() -> Vec<u8> {
    std::fs::read(specforge_component::committed::GREET.path())
        .expect("the greet fixture is vendored")
}

/// The greet blob with its declared name (`@sdk/greet`, 10 bytes) replaced
/// by another name of the same length.
pub(crate) fn greet_named(name: &str) -> Vec<u8> {
    let (from, to) = (b"@sdk/greet".as_slice(), name.as_bytes());
    assert_eq!(from.len(), to.len());
    let mut wasm = greet_wasm();
    let mut at = 0;
    while at + from.len() <= wasm.len() {
        if &wasm[at..at + from.len()] == from {
            wasm[at..at + from.len()].copy_from_slice(to);
            at += from.len();
        } else {
            at += 1;
        }
    }
    wasm
}

/// A project whose only registry is `registry`.
pub(crate) fn project_on(registry: &LocalRegistry) -> TestProject {
    TestProject::new()
        .enabling(&["@specforge/software"])
        .config(|c| {
            c["name"] = "p".into();
            c["registries"] = registry.config_entry();
        })
}
