//! An in-process adapter of the [`WasmRuntime`] seam, for tests: it serves
//! an extension straight from its SDK [`ContributionsBuilder`], routed by
//! the guest's own [`GuestServed`](specforge_extension_sdk::Served) — the function `component_guest!` calls in
//! a component — so a test declares an extension with the same builders an
//! extension author uses, and the host loads it exactly as it loads a
//! component.
//!
//! It is a test adapter: the "guest" runs in the host process, with no
//! sandbox, no fuel and no deadline (it records the limits the host applies).
//! Sandbox obligations stay proven only through the component runtime.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use specforge_extension_sdk::{
    ContributionsBuilder, ExportHandler, Served as GuestServed, no_other_exports,
};

use crate::runtime::{WasmCallResult, WasmRuntime, WasmTrapInfo};
use crate::sandbox::Limits;

/// What builds an extension's contributions, per call (as its guest does).
type Build = Arc<dyn Fn() -> ContributionsBuilder + Send + Sync>;

/// An extension served under a name: what builds it, and what answers the
/// exports its declarations do not.
type Served = (Build, ExportHandler);

/// One call the runtime answered.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub extension: String,
    pub export: String,
    /// The input as JSON (a string of the bytes when they are not JSON).
    pub input: serde_json::Value,
}

/// An answer given instead of routing a call: to one export, or to one
/// export called with one input.
type Overrides = Vec<(String, String, Option<serde_json::Value>, WasmCallResult)>;

/// Serves SDK-declared extensions in process; see the module docs.
#[derive(Default)]
pub struct InProcessRuntime {
    /// The extensions served, by the name each is loaded as. Behind a lock:
    /// loading, renaming and unloading change it through the port.
    extensions: Mutex<BTreeMap<String, Served>>,
    /// Binaries that serve an extension under whatever name they are
    /// loaded as ([`InProcessRuntime::binary`]).
    binaries: Vec<(Vec<u8>, Served)>,
    overrides: Mutex<Overrides>,
    faults: Vec<(String, String)>,
    calls: Mutex<Vec<RecordedCall>>,
    limits: Mutex<Vec<(String, Limits)>>,
}

impl InProcessRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Serve the extension `build` declares (named by its `meta.name`),
    /// routed as its guest is, with no exports beyond its declarations.
    pub fn with(self, build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static) -> Self {
        self.with_handler(build, no_other_exports)
    }

    /// Serve the extension `build` declares, with `handler` answering the
    /// exports no declaration answers (`component_guest!`'s `handler`).
    pub fn with_handler(
        self,
        build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static,
        handler: ExportHandler,
    ) -> Self {
        let name = build().meta.name.clone();
        self.serving(&name, build, handler)
    }

    /// Serve the extension `build` declares under `name`, the name the
    /// host loads it by, which need not be the one it declares (a binary
    /// installed from a path), with `handler` as in
    /// [`InProcessRuntime::with_handler`].
    pub fn serving(
        mut self,
        name: &str,
        build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static,
        handler: ExportHandler,
    ) -> Self {
        self.extensions
            .get_mut()
            .expect("extensions lock")
            .insert(name.to_string(), (Arc::new(build), handler));
        self
    }

    /// Serve `build` under whatever name a module of exactly `bytes` is
    /// loaded as: a `.wasm` file whose content a test chooses, installed
    /// and loaded as any extension is.
    pub fn binary(
        mut self,
        bytes: &[u8],
        build: impl Fn() -> ContributionsBuilder + Send + Sync + 'static,
    ) -> Self {
        self.binaries
            .push((bytes.to_vec(), (Arc::new(build), no_other_exports)));
        self
    }

    /// Answer `export` of `extension` with `result` (raw bytes or a trap)
    /// instead of routing it: for failure-mapping tests. The extension
    /// need not be served otherwise.
    pub fn answer_raw(self, extension: &str, export: &str, result: WasmCallResult) -> Self {
        self.overriding(extension, export, None, result)
    }

    /// Answer `export` of `extension` with `result` when it is called with
    /// `input` (compared as JSON), and route its other calls: one describe
    /// category answering what its declaration could not hold.
    pub fn answer_raw_to(
        self,
        extension: &str,
        export: &str,
        input: serde_json::Value,
        result: WasmCallResult,
    ) -> Self {
        self.overriding(extension, export, Some(input), result)
    }

    fn overriding(
        self,
        extension: &str,
        export: &str,
        input: Option<serde_json::Value>,
        result: WasmCallResult,
    ) -> Self {
        self.overrides.lock().expect("overrides lock").push((
            extension.to_string(),
            export.to_string(),
            input,
            result,
        ));
        self
    }

    /// Make calling `export` of `extension` panic in the host, outside the
    /// guest: a fault of the runtime itself (a broken host function), not a
    /// guest trap, for tests of what the host does when its runtime fails.
    pub fn fault(mut self, extension: &str, export: &str) -> Self {
        self.faults
            .push((extension.to_string(), export.to_string()));
        self
    }

    /// Every call answered so far, in order.
    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().expect("calls lock").clone()
    }

    /// Forget the calls answered so far.
    pub fn clear_calls(&self) {
        self.calls.lock().expect("calls lock").clear();
    }

    /// Every limit the host applied (`(extension, limits)`), in order. The
    /// in-process runtime records them; it enforces none.
    pub fn limits(&self) -> Vec<(String, Limits)> {
        self.limits.lock().expect("limits lock").clone()
    }
}

fn trap(kind: &str, message: String, export: &str) -> WasmCallResult {
    WasmCallResult::Trap(WasmTrapInfo {
        kind: kind.to_string(),
        message,
        export_name: export.to_string(),
    })
}

impl WasmRuntime for InProcessRuntime {
    /// A binary this runtime was given ([`InProcessRuntime::binary`]) is
    /// served under `name`; a name it serves, or answers raw
    /// ([`InProcessRuntime::answer_raw`]), loads as it is (an extension
    /// served in process is whatever binary is installed under its name);
    /// any other bytes are no component it serves.
    fn load(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let mut extensions = self.extensions.lock().expect("extensions lock");
        if let Some((_, served)) = self.binaries.iter().find(|(known, _)| known == bytes) {
            extensions.insert(name.to_string(), served.clone());
            return Ok(());
        }
        let answered = self
            .overrides
            .lock()
            .expect("overrides lock")
            .iter()
            .any(|(extension, ..)| extension == name);
        if extensions.contains_key(name) || answered {
            return Ok(());
        }
        Err(format!(
            "'{name}' is not a component the in-process runtime serves"
        ))
    }

    fn rename(&self, from: &str, to: &str) -> bool {
        let mut extensions = self.extensions.lock().expect("extensions lock");
        match extensions.remove(from) {
            Some(served) => {
                extensions.insert(to.to_string(), served);
                true
            }
            None => false,
        }
    }

    fn unload(&self, name: &str) -> bool {
        self.extensions
            .lock()
            .expect("extensions lock")
            .remove(name)
            .is_some()
    }

    fn apply_limits(&self, extension: &str, limits: Limits) {
        self.limits
            .lock()
            .expect("limits lock")
            .push((extension.to_string(), limits));
    }

    fn call_export(&self, extension: &str, export: &str, input: &[u8]) -> WasmCallResult {
        let input_json: serde_json::Value = serde_json::from_slice(input)
            .unwrap_or_else(|_| String::from_utf8_lossy(input).into_owned().into());
        self.calls.lock().expect("calls lock").push(RecordedCall {
            extension: extension.to_string(),
            export: export.to_string(),
            input: input_json.clone(),
        });
        if self
            .faults
            .iter()
            .any(|(e, x)| e == extension && x == export)
        {
            panic!("the runtime failed calling {export} of {extension}");
        }
        let overridden = self
            .overrides
            .lock()
            .expect("overrides lock")
            .iter()
            .find(|(e, x, when, _)| {
                e == extension && x == export && when.as_ref().is_none_or(|w| *w == input_json)
            })
            .map(|(_, _, _, result)| result.clone());
        if let Some(result) = overridden {
            return result;
        }
        let served = self
            .extensions
            .lock()
            .expect("extensions lock")
            .get(extension)
            .cloned();
        let Some((build, handler)) = served else {
            return trap(
                "extension_not_found",
                format!("Extension '{extension}' not loaded"),
                export,
            );
        };
        // A guest panic is a trap, as `unreachable` is in a component.
        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            GuestServed::new(build()).call(handler, export, input)
        }));
        match answer {
            Ok(Ok(bytes)) => WasmCallResult::Ok(bytes),
            Ok(Err(message)) => trap("guest_error", message, export),
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                trap("call_failed", format!("unreachable: {message}"), export)
            }
        }
    }
}

/// The contract every adapter of the [`WasmRuntime`] port keeps, asserted
/// over `runtime`, which must serve `extension` (an SDK-built guest) with
/// `panicking` an export whose handler panics:
///
/// - a call to an extension the runtime did not load is the trap
///   `extension_not_found`;
/// - an export the guest does not route is the trap `guest_error`
///   `unknown export '<name>'`, as the guest's own routing answers it;
/// - a handler's error is the trap `guest_error` carrying its message (the
///   guest's `__describe` of a category the protocol does not have);
/// - a guest that panics is a trap (`call_failed`), never a host crash;
/// - the runtime still answers the extension afterwards (its handshake).
///
/// The component runtime and [`InProcessRuntime`] run it in their tests,
/// so the two adapters cannot disagree on what a failure looks like.
pub fn assert_runtime_contract(runtime: &dyn WasmRuntime, extension: &str, panicking: &str) {
    let trap = |result: WasmCallResult, what: &str| match result {
        WasmCallResult::Trap(trap) => trap,
        WasmCallResult::Ok(bytes) => panic!(
            "{what}: answered {} instead of trapping",
            String::from_utf8_lossy(&bytes)
        ),
    };

    let missing = trap(
        runtime.call_export("@contract/not-loaded", "__handshake", b"{}"),
        "an extension that is not loaded",
    );
    assert_eq!(missing.kind, "extension_not_found", "{missing:?}");
    assert_eq!(missing.export_name, "__handshake");

    let unrouted = trap(
        runtime.call_export(extension, "contract__no_such_export", b"{}"),
        "an export the guest does not route",
    );
    assert_eq!(unrouted.kind, "guest_error", "{unrouted:?}");
    assert_eq!(
        unrouted.message,
        "unknown export 'contract__no_such_export'"
    );
    assert_eq!(unrouted.export_name, "contract__no_such_export");

    let refused = trap(
        runtime.call_export(
            extension,
            "__describe",
            br#"{"category":"no_such_category"}"#,
        ),
        "a handler's error",
    );
    assert_eq!(refused.kind, "guest_error", "{refused:?}");
    assert_eq!(refused.message, "unsupported category: no_such_category");

    let panicked = trap(
        runtime.call_export(extension, panicking, b"{}"),
        "a guest that panics",
    );
    assert_eq!(panicked.kind, "call_failed", "{panicked:?}");
    assert_eq!(panicked.export_name, panicking);

    match runtime.call_export(extension, "__handshake", b"{}") {
        WasmCallResult::Ok(bytes) => {
            let handshake: serde_json::Value =
                serde_json::from_slice(&bytes).expect("a handshake is JSON");
            assert_eq!(handshake["name"], extension);
        }
        WasmCallResult::Trap(trap) => panic!("the extension stopped answering: {trap:?}"),
    }
}

/// The contract every adapter of the [`WasmRuntime`] port keeps for the
/// modules it holds, asserted over `runtime`, which must load `bytes` as a
/// component declaring `declared`:
///
/// - loading `bytes` under a name registers them there, and the extension
///   answers its handshake under that name;
/// - a renamed module answers under its new name and no longer under the
///   old one; renaming what is not loaded is false;
/// - an unloaded module is not loaded any more; unloading twice is false.
pub fn assert_module_contract(runtime: &dyn WasmRuntime, bytes: &[u8], declared: &str) {
    let answers = |name: &str| match runtime.call_export(name, "__handshake", b"{}") {
        WasmCallResult::Ok(answer) => {
            let handshake: serde_json::Value =
                serde_json::from_slice(&answer).expect("a handshake is JSON");
            assert_eq!(handshake["name"], declared);
            true
        }
        WasmCallResult::Trap(trap) => {
            assert_eq!(trap.kind, "extension_not_found", "{trap:?}");
            false
        }
    };

    assert!(!answers("loaded-as"), "nothing is loaded yet");
    runtime
        .load("loaded-as", bytes)
        .expect("the bytes are a component");
    assert!(answers("loaded-as"), "loading registers the bytes by name");

    assert!(!runtime.rename("not-loaded", "elsewhere"));
    assert!(runtime.rename("loaded-as", "renamed"));
    assert!(
        answers("renamed"),
        "a renamed module answers as its new name"
    );
    assert!(!answers("loaded-as"), "and no longer as the old one");

    assert!(runtime.unload("renamed"));
    assert!(!answers("renamed"), "an unloaded module is not loaded");
    assert!(!runtime.unload("renamed"), "nothing is left to unload");
}
