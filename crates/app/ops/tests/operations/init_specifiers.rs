//! What `init` does with two specifiers that name one extension: the rule `add` applies
//! (`extension::claims`, ADR 0098).

use std::path::Path;

use specforge_ops::authoring::init::{self, InitField};
use specforge_ops::testing::{candidates, declaring, serving_builtin};
use specforge_ops::{OpErrorKind, Refusal};
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

const A_ONE: &[u8] = b"\0asm in-process @x/a 1.0.0 one";
const A_TWO: &[u8] = b"\0asm in-process @x/a 1.0.0 two";

/// Two binaries that declare `@x/a` 1.0.0 with other bytes.
fn runtime() -> InProcessRuntime {
    serving_builtin(
        candidates(),
        "@specforge/product",
        declaring("@specforge/product", "1.0.0", &[]),
    )
    .binary(A_ONE, declaring("@x/a", "1.0.0", &[]))
    .binary(A_TWO, declaring("@x/a", "1.0.0", &[]))
}

fn init_with(
    dir: &Path,
    extensions: &[String],
    runtime: &InProcessRuntime,
) -> Result<init::Outcome, Refusal<InitField>> {
    init::init(
        &init::Request {
            dir,
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions,
        },
        runtime,
    )
}

#[test]
#[ignore = "flipped by T4"]
fn init_refuses_two_local_files_that_add_one_extension_differently() {
    let scratch = TempDir::new().unwrap();
    let (a, b) = (scratch.path().join("a.wasm"), scratch.path().join("b.wasm"));
    std::fs::write(&a, A_ONE).unwrap();
    std::fs::write(&b, A_TWO).unwrap();
    let dir = scratch.path().join("demo");
    let extensions = vec![a.display().to_string(), b.display().to_string()];

    let error = init_with(&dir, &extensions, &runtime()).unwrap_err();

    assert_eq!(error.error.code, "extension_conflict");
    assert_eq!(error.error.kind, OpErrorKind::Conflict);
    assert_eq!(error.subject, Some(InitField::Extensions));
    let message = &error.error.message;
    assert!(
        message.starts_with(&format!(
            "'{}' and '{}' both add '@x/a', as 1.0.0 from local:",
            a.display(),
            b.display()
        )),
        "{message}"
    );
    assert!(message.contains(" and as 1.0.0 from local:"), "{message}");
    assert_eq!(error.error.suggestion.as_deref(), Some("add '@x/a' once"));
    assert!(!dir.exists(), "nothing was written");
}

#[test]
fn init_enables_once_an_extension_two_specifiers_name_the_same_way() {
    let scratch = TempDir::new().unwrap();
    let a = scratch.path().join("a.wasm");
    std::fs::write(&a, A_ONE).unwrap();
    std::fs::create_dir(scratch.path().join("sub")).unwrap();
    let again = scratch.path().join("sub/../a.wasm");
    let dir = scratch.path().join("demo");
    let extensions = vec![
        a.display().to_string(),
        again.display().to_string(),
        "@specforge/product".to_string(),
        "@specforge/product@latest".to_string(),
    ];

    let outcome = init_with(&dir, &extensions, &runtime()).unwrap();

    let count = |name: &str| outcome.extensions.iter().filter(|n| *n == name).count();
    assert_eq!(count("@x/a"), 1, "{:?}", outcome.extensions);
    assert_eq!(count("@specforge/product"), 1, "{:?}", outcome.extensions);
    let lock = std::fs::read_to_string(dir.join("specforge.lock")).unwrap();
    assert_eq!(lock.matches("\"@x/a\"").count(), 1, "{lock}");
}
