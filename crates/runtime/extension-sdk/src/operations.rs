//! Operational contributions declared with their handlers: compiler
//! passes, collectors, custom rules, scanners and the migration hook.
//!
//! As for surfaces (ADR 0011), one declaration gives both halves: the
//! descriptor the host loads and the routing of the export the host then
//! calls (`__pass_<name>`, `collect__<name>`, the rule's `wasm_function`,
//! the analyzer's `scan_export`, the hook's name) to the handler, which
//! reads the protocol's input type and answers its answer type (ADR 0013).
//! A declaration cannot lack its code: declaring one without its handler
//! panics when the extension is built.
//!
//! Each handler is wrapped, where it is declared, in the code that decodes
//! its input and encodes its answer, so a guest links the wire code of the
//! operations it declares and no other.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// An export's answer from its raw input.
pub(crate) type Wire = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, String>>;

/// `input` decoded as `I`; the error names `what` (`invalid <what> input: …`).
pub(crate) fn decode<I: DeserializeOwned>(what: &str, input: &[u8]) -> Result<I, String> {
    serde_json::from_slice(input).map_err(|e| format!("invalid {what} input: {e}"))
}

/// `answer` encoded; the error names `what` (`<what> answer did not encode: …`).
pub(crate) fn encode<O: Serialize>(what: &str, answer: &O) -> Result<Vec<u8>, String> {
    serde_json::to_vec(answer).map_err(|e| format!("{what} answer did not encode: {e}"))
}

/// `handler` as an export: its input decoded as `I` (`what` names it in
/// the error when it does not decode), its answer encoded.
pub(crate) fn wire<I, O>(
    what: &'static str,
    handler: impl Fn(&I) -> Result<O, String> + 'static,
) -> Wire
where
    I: DeserializeOwned,
    O: Serialize,
{
    Box::new(move |input| encode(what, &handler(&decode(what, input)?)?))
}

/// Every operational export an extension declared, with its handler, in
/// declaration order.
#[derive(Default)]
pub(crate) struct Operations {
    exports: Vec<(String, String, Wire)>,
}

impl Operations {
    /// Route `export` to `wire`; `what` names the declaration in a panic.
    ///
    /// # Panics
    ///
    /// When a declared operation already answers `export`.
    pub(crate) fn add(&mut self, export: String, what: String, wire: Wire) {
        if let Some((_, other, _)) = self.exports.iter().find(|(e, _, _)| *e == export) {
            panic!("{what}'s export {export} is already {other}'s");
        }
        self.exports.push((export, what, wire));
    }

    /// Who answers `export`, if a declared operation does.
    pub(crate) fn owner(&self, export: &str) -> Option<&str> {
        self.exports
            .iter()
            .find(|(e, _, _)| e == export)
            .map(|(_, what, _)| what.as_str())
    }

    /// The wire answer of `export`; `None` when no declared operation has
    /// it.
    pub(crate) fn dispatch(&self, export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
        self.exports
            .iter()
            .find(|(e, _, _)| e == export)
            .map(|(_, _, wire)| wire(input))
    }
}
