use crate::{ContributionsBuilder, ExtensionDeclaration, describe_answer, operations};
use specforge_protocol_types::DescribeRequest;
use std::borrow::Cow;

/// An extension as its component guest serves the host: its builder, and
/// the declaration and handshake read from it once. [`component_guest!`](crate::component_guest)
/// keeps one per instance.
pub struct Served {
    builder: ContributionsBuilder,
    declaration: ExtensionDeclaration,
    handshake: Vec<u8>,
}

impl Served {
    /// Serve `builder`: its declaration and handshake are built here, once.
    pub fn new(builder: ContributionsBuilder) -> Self {
        let declaration = builder.declaration();
        let handshake = declaration.handshake_json().into_bytes();
        Self {
            builder,
            declaration,
            handshake,
        }
    }

    /// The answer to the host's `call(name, export, input)`:
    /// - `__handshake` and `__describe` from the declaration read once;
    /// - a declared surface's or operation's export by its handler
    ///   ([`ContributionsBuilder::dispatch_export`]);
    /// - any other export through `handler`, whose `None` is
    ///   `unknown export '<name>'`, as a missing export is.
    pub fn call(
        &self,
        handler: ExportHandler,
        export: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        match export {
            "__handshake" => Ok(self.handshake.clone()),
            "__describe" => {
                let request: DescribeRequest = serde_json::from_slice(input)
                    .map_err(|e| format!("invalid describe request: {e}"))?;
                match describe_answer(
                    &self.builder.raw,
                    || Cow::Borrowed(&self.declaration),
                    &request.category,
                ) {
                    Some(body) => Ok(body.into_bytes()),
                    None => Err(format!("unsupported category: {}", request.category)),
                }
            }
            other => match self.builder.dispatch_export(other, input) {
                Some(result) => result,
                None => match handler(other, input) {
                    Some(result) => result,
                    None => Err(format!("unknown export '{other}'")),
                },
            },
        }
    }
}

/// The `handler` of a [`component_guest!`](crate::component_guest): the answer to an export no
/// declaration answers, `None` for a name it does not implement.
pub type ExportHandler = fn(&str, &[u8]) -> Option<Result<Vec<u8>, String>>;

/// The answer of an export the guest's `handler` serves (one no builder
/// declares with its handler: an analyzer's `classify__`/`map__`, or an
/// export of a category given with [`ContributionsBuilder::raw_category`]):
/// `input` decoded as `I`, `handler`'s answer encoded. An input that is not
/// an `I` is the guest's error, naming the export.
pub fn answer_export<I, O>(
    export: &str,
    input: &[u8],
    handler: impl FnOnce(&I) -> O,
) -> Result<Vec<u8>, String>
where
    I: serde::de::DeserializeOwned,
    O: serde::Serialize,
{
    let input: I = operations::decode(export, input)?;
    operations::encode(export, &handler(&input))
}

/// The `handler` of a [`component_guest!`](crate::component_guest) that names none: no export
/// beyond the protocol's and the declared surfaces' and operations'.
pub fn no_other_exports(_export: &str, _input: &[u8]) -> Option<Result<Vec<u8>, String>> {
    None
}

/// Component-mode export glue (wasm32-wasip2 guests).
///
/// Expands to the bindings of the `specforge:bridge@0.1.0` world (through the
/// SDK's own `wit-bindgen`, so the guest needs no dependency of its own) plus
/// a `Guest` implementation that
/// dispatches the host's `call(name, export-name, input)` entry point:
///
/// - `__handshake` / `__describe` are served from the [`ContributionsBuilder`]
///   produced by `$build` (the same JSON wire protocol as always);
/// - a declared surface's export (a command, an MCP tool, an MCP resource:
///   [`ContributionsBuilder::command`] and its siblings) is answered by its
///   declared handler ([`ContributionsBuilder::dispatch_export`]);
/// - every other export name is forwarded to `$handler`, which returns
///   `None` for names it does not implement (the guest then errors, exactly
///   like a missing export). An extension whose only exports are its
///   surfaces leaves `handler` out.
///
/// ```ignore
/// fn build() -> specforge_extension_sdk::ContributionsBuilder { /* ... */ }
/// fn dispatch(export: &str, input: &[u8]) -> Option<Result<Vec<u8>, String>> {
///     match export {
///         "scan__rust" => Some(serde_json::to_vec(&scan_rust(req)?)),
///         _ => None,
///     }
/// }
/// specforge_extension_sdk::component_guest!(build = build, handler = dispatch);
/// ```
#[macro_export]
macro_rules! component_guest {
    (build = $build:expr) => {
        ::specforge_extension_sdk::component_guest!(
            build = $build,
            handler = ::specforge_extension_sdk::no_other_exports
        );
    };
    (build = $build:expr, handler = $handler:expr) => {
        ::specforge_extension_sdk::__wit_bindgen::generate!({
            inline: r#"
package specforge:bridge@0.1.0;

world bridge {
    export call: func(name: string, export-name: string, input: list<u8>) ->
        result<list<u8>, string>;
}
"#,
            runtime_path: "::specforge_extension_sdk::__wit_bindgen::rt",
        });

        struct BridgeGuest;

        impl Guest for BridgeGuest {
            fn call(
                _name: String,
                export_name: String,
                input: Vec<u8>,
            ) -> Result<Vec<u8>, String> {
                // A component instance is single-threaded, and the host makes
                // a new one after a trap, so the builder, its declaration and
                // the handshake are built once per instance.
                ::std::thread_local! {
                    static SERVED: ::specforge_extension_sdk::Served =
                        ::specforge_extension_sdk::Served::new($build());
                }
                SERVED.with(|served| served.call($handler, &export_name, &input))
            }
        }

        export!(BridgeGuest);
    };
}
