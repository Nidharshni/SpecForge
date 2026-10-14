//! The sandbox an extension runs in (ADR 0037).
//!
//! A component is granted no capability, whatever it declares: no preopened
//! directory, environment, arguments or stdin, no socket and no name lookup
//! (the component runtime builds that context; `sandbox_probe` proves it).
//! What an extension declares in its handshake's `sandbox_policy` is limits
//! only — a call's wall-clock budget and its instance's linear memory — each
//! held to the host's ceiling. The loader applies them, once the handshake's
//! protocol major is checked
//! ([`load_declaration`](crate::protocol::load_declaration) through
//! [`WasmRuntime::apply_limits`](crate::runtime::WasmRuntime::apply_limits)); a
//! [`handshake`](crate::ExtensionCalls::handshake) call on its own applies
//! none. What the declaration asks for that the host does not honour is W153.

use serde_json::Value;
use specforge_common::{Diagnostic, codes};
use specforge_protocol_types::UnknownKey;

/// What the host holds every call into one extension to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wall-clock budget of one call, in milliseconds (epoch interruption;
    /// never interrupted before it has elapsed).
    pub execution_ms: u32,
    /// Ceiling of the instance's linear memory, in MiB: a growth past it
    /// traps the call.
    pub memory_mb: u32,
}

impl Limits {
    /// The host's ceiling: the limits of an extension that declares none,
    /// and what no declaration exceeds (an extension may tighten its
    /// sandbox, never widen it).
    pub const CEILING: Limits = Limits {
        execution_ms: 30_000,
        memory_mb: 512,
    };
}

/// One extension's sandbox: the limits it runs under, and what its
/// declaration asks for that the host does not honour.
#[derive(Debug, Clone, PartialEq)]
pub struct Sandbox {
    pub limits: Limits,
    /// W153, one per key, by key name (an object's key order is not
    /// meaningful on the wire).
    pub unhonoured: Vec<Diagnostic>,
}

/// The two limits a `sandbox_policy` may declare.
const MAX_EXECUTION_MS: &str = "max_execution_ms";
const MAX_MEMORY_MB: &str = "max_memory_mb";
const LIMIT_KEYS: [&str; 2] = [MAX_EXECUTION_MS, MAX_MEMORY_MB];

/// The permission fields a guest built before ADR 0037 declares.
const PERMISSION_KEYS: [&str; 5] = [
    "allowed_domains",
    "allowed_paths",
    "allowed_output_extensions",
    "network_access",
    "file_system_access",
];

/// W153 for a surface item's `sandbox` key, the override a guest built
/// before ADR 0037 declares; `None` for any other unknown describe key
/// (which stays W138).
pub fn unhonoured_surface_key(extension: &str, key: &UnknownKey) -> Option<Diagnostic> {
    let on_a_surface_export = key.category == "surfaces"
        && key.key == "sandbox"
        && ["commands[", "mcp_tools[", "mcp_resources["]
            .iter()
            .any(|list| key.item.contains(&format!(".{list}")))
        && key.item.ends_with(']');
    on_a_surface_export.then(|| {
        Diagnostic::new(
            codes::W153,
            format!(
                "extension '{extension}': surface '{}' declares a sandbox override, which grants \
                 nothing (every surface export runs with no capability); it is ignored",
                key.item
            ),
        )
        .with_suggestion(
            "remove the key: a component gets no file, network, environment or stdin access \
             (ADR 0037)",
        )
    })
}

impl Sandbox {
    /// The sandbox of `extension`, whose handshake answered `sandbox_policy`
    /// as sent (`None` when absent or `null`). Each declared limit
    /// (`max_execution_ms`, `max_memory_mb`) is applied as declared, held to
    /// [`Limits::CEILING`] (W153 when above it); an undeclared one is the
    /// ceiling. Any other key whose value asks for something (`true`, a
    /// non-zero number, a non-empty string, array or object) is W153: the
    /// host grants no capability. The handshake already decoded, so the
    /// limits are well-typed here.
    pub fn of(extension: &str, sandbox_policy: Option<&Value>) -> Sandbox {
        let declared = |key: &str| {
            sandbox_policy
                .and_then(|policy| policy.get(key))
                .and_then(Value::as_u64)
        };
        let held = |declared: Option<u64>, ceiling: u32| {
            declared.map_or(ceiling, |value| {
                u32::try_from(value).map_or(ceiling, |value| value.min(ceiling))
            })
        };
        let limits = Limits {
            execution_ms: held(declared(MAX_EXECUTION_MS), Limits::CEILING.execution_ms),
            memory_mb: held(declared(MAX_MEMORY_MB), Limits::CEILING.memory_mb),
        };

        let mut unhonoured = Vec::new();
        if let Some(policy) = sandbox_policy.and_then(Value::as_object) {
            let mut keys: Vec<&String> = policy.keys().collect();
            keys.sort();
            for key in keys {
                let value = &policy[key];
                if LIMIT_KEYS.contains(&key.as_str()) {
                    let ceiling = if key == MAX_EXECUTION_MS {
                        Limits::CEILING.execution_ms
                    } else {
                        Limits::CEILING.memory_mb
                    };
                    if let Some(asked) = value.as_u64().filter(|asked| *asked > u64::from(ceiling))
                    {
                        unhonoured.push(above_ceiling(extension, key, asked, ceiling));
                    }
                } else if asks_for_something(value) {
                    unhonoured.push(never_granted(extension, key));
                }
            }
        }
        Sandbox { limits, unhonoured }
    }
}

/// Whether a policy key's value asks for anything: `true`, a number other
/// than 0, a non-empty string, array or object. `false`, `0`, `""`, `[]`,
/// `{}` and `null` ask for nothing, which keeps an older guest that
/// serialized `network_access: false, allowed_paths: []` quiet.
fn asks_for_something(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(asked) => *asked,
        Value::Number(number) => number.as_f64() != Some(0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
    }
}

/// W153: a declared limit above the host's ceiling, held to it.
fn above_ceiling(extension: &str, key: &str, declared: u64, ceiling: u32) -> Diagnostic {
    let held = if key == MAX_EXECUTION_MS {
        format!("its calls are held to {ceiling} ms")
    } else {
        format!("its instance is held to {ceiling} MB")
    };
    Diagnostic::new(
        codes::W153,
        format!(
            "extension '{extension}': sandbox_policy.{key} is {declared}, above the host's \
             ceiling of {ceiling}; {held}"
        ),
    )
    .with_suggestion(format!(
        "declare at most {ceiling}, or remove the limit to get the ceiling"
    ))
}

/// W153: a policy key the host does not read, whose value asks for something.
fn never_granted(extension: &str, key: &str) -> Diagnostic {
    if PERMISSION_KEYS.contains(&key) {
        Diagnostic::new(
            codes::W153,
            format!(
                "extension '{extension}': sandbox_policy.{key} asks for a capability the host \
                 never grants; it is ignored"
            ),
        )
        .with_suggestion(
            "remove the key: a component gets no file, network, environment or stdin access \
             (ADR 0037)",
        )
    } else {
        Diagnostic::new(
            codes::W153,
            format!(
                "extension '{extension}': sandbox_policy.{key} is not a limit the host reads; \
                 it is ignored"
            ),
        )
        .with_suggestion(format!(
            "check the key's spelling: the limits are {MAX_EXECUTION_MS} and {MAX_MEMORY_MB} \
             (ADR 0037)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use specforge_protocol_types::SandboxPolicy;
    use specforge_test_macros::test as specforge_test;

    fn warnings(policy: Value) -> Vec<Diagnostic> {
        Sandbox::of("@acme/x", Some(&policy)).unhonoured
    }

    #[test]
    fn limits_are_held_to_the_ceiling() {
        let policy = |value: Value| Sandbox::of("@acme/x", Some(&value)).limits;
        assert_eq!(
            policy(json!({"max_execution_ms": 60000, "max_memory_mb": 1024})),
            Limits::CEILING
        );
        assert_eq!(
            policy(json!({"max_execution_ms": 50})),
            Limits {
                execution_ms: 50,
                memory_mb: 512
            }
        );
        assert_eq!(
            policy(json!({"max_execution_ms": null, "max_memory_mb": 256})),
            Limits {
                execution_ms: 30_000,
                memory_mb: 256
            }
        );
        assert_eq!(Sandbox::of("@acme/x", None).limits, Limits::CEILING);
    }

    /// The policy type declares exactly the limits the sandbox reads: a field
    /// added to the type without a rule here fails the build.
    #[test]
    fn limit_keys_are_the_policy_fields() {
        let written = serde_json::to_value(SandboxPolicy {
            max_execution_ms: Some(1),
            max_memory_mb: Some(1),
        })
        .unwrap();
        let mut keys: Vec<&str> = written
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut limit_keys = LIMIT_KEYS;
        limit_keys.sort_unstable();
        assert_eq!(keys, limit_keys);
    }

    #[specforge_test(
        behavior = "configure_sandbox_policy",
        verify = "a declared limit above the ceiling is held to it, with W153"
    )]
    fn above_ceiling_is_w153() {
        let found = warnings(json!({"max_memory_mb": 1024}));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, "W153");
        for part in ["'@acme/x'", "max_memory_mb", "1024", "512"] {
            assert!(
                found[0].message.contains(part),
                "{part}: {}",
                found[0].message
            );
        }
        let found = warnings(json!({"max_execution_ms": 60000}));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].message.contains("30000 ms"),
            "{}",
            found[0].message
        );
        // At the ceiling is not above it.
        assert!(warnings(json!({"max_memory_mb": 512, "max_execution_ms": 30000})).is_empty());
    }

    #[specforge_test(
        behavior = "configure_sandbox_policy",
        verify = "a sandbox_policy key that asks for a capability is W153"
    )]
    fn a_key_asking_for_a_capability_is_w153() {
        let found = warnings(json!({
            "network_access": true,
            "allowed_paths": ["/etc"],
            "max_memroy_mb": 64,
        }));
        let keys: Vec<&str> = found
            .iter()
            .map(|diagnostic| {
                ["allowed_paths", "max_memroy_mb", "network_access"]
                    .into_iter()
                    .find(|key| {
                        diagnostic
                            .message
                            .contains(&format!("sandbox_policy.{key} "))
                    })
                    .expect("each warning names its key")
            })
            .collect();
        assert_eq!(keys, ["allowed_paths", "max_memroy_mb", "network_access"]);
        assert!(found.iter().all(|diagnostic| diagnostic.code == "W153"));
        // The misspelled limit is not a capability, and the message says so.
        assert!(found[1].message.contains("not a limit the host reads"));
    }

    #[specforge_test(
        behavior = "configure_sandbox_policy",
        verify = "a sandbox_policy key that asks for nothing is not reported"
    )]
    fn a_key_asking_for_nothing_is_not_reported() {
        assert!(
            warnings(json!({
                "network_access": false,
                "file_system_access": false,
                "allowed_domains": [],
                "allowed_output_extensions": [],
                "allowed_paths": null,
                "max_memory_mb": null,
            }))
            .is_empty()
        );
        assert!(Sandbox::of("@acme/x", None).unhonoured.is_empty());
        assert!(
            Sandbox::of("@acme/x", Some(&json!(null)))
                .unhonoured
                .is_empty()
        );
    }
}
