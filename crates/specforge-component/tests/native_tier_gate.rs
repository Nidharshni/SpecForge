//! The native execution tier — `BuiltinRuntime`, `BuiltinExtension`, the
//! in-emitter builtins, `NativeCustomRules`, `CompositeRuntime`, and
//! `runtime_for_extensions` — was deleted. This test fails if any of those
//! symbols reappear anywhere in workspace source, so a future change cannot
//! silently reintroduce a first-party native tier (which would violate R-1:
//! all plugins are the same kind, executed through the same Wasm runtime).

use std::path::{Path, PathBuf};

const FORBIDDEN_SYMBOLS: &[&str] = &[
    "BuiltinRuntime",
    "BuiltinExtension",
    "NativeCustomRules",
    "CompositeRuntime",
    "runtime_for_extensions",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn no_native_tier_symbols_in_workspace_source() {
    let root = workspace_root();
    let mut offenders = Vec::new();

    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if matches!(name, "target" | ".git" | "node_modules" | "docs" | "spec") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && path.file_name().and_then(|n| n.to_str()) != Some("native_tier_gate.rs")
            {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for symbol in FORBIDDEN_SYMBOLS {
                    if content.contains(symbol) {
                        offenders.push(format!("{} references {}", path.display(), symbol));
                    }
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "native-tier symbols reappeared:\n{}",
        offenders.join("\n")
    );
    // Sanity: the gate actually walks source (it must be able to find THIS file).
    assert!(workspace_root().join("crates").exists());
}
