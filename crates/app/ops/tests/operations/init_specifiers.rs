//! What `init` does with two specifiers that name one extension: the rule `add` applies
//! (`extension::claims`, ADR 0098).

use std::path::Path;

use crate::writes::{candidates as writes_candidates, files_under, project};

use specforge_ops::authoring::init::{self, InitField};
use specforge_ops::extension::{self, AddRequest};
use specforge_ops::registry::Trust;
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

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init enables an extension two of its specifiers name the same way once, and refuses two that add it differently with extension_conflict, as add does"
)]
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

#[specforge_test_macros::test(
    behavior = "non_interactive_init",
    verify = "init reads its extension specifiers as add reads them, with add's codes"
)]
fn init_reads_its_specifiers_as_add_reads_them() {
    use specforge_ops::authoring::init;
    let scratch = TempDir::new().unwrap();
    let missing = scratch.path().join("missing.wasm").display().to_string();
    let cases = [
        ("Bad Name", "E054"),
        ("@acme/x@>>1", "R-RES-003"),
        (missing.as_str(), "E054"),
        ("git+https://x/y", "E064"),
        ("@acme/x", "E063"),
    ];
    for (specifier, code) in cases {
        let existing = project(&[], &[]);
        let before = files_under(existing.path());
        let by_add = extension::specified(&[specifier]).and_then(|extensions| {
            extension::add(
                &AddRequest {
                    root: &specforge_common::ProjectRoot::of(existing.path()).unwrap(),
                    extensions,
                    allow_unsigned: false,
                    trust: Trust::Refuse,
                    dry_run: false,
                },
                &specforge_ops::registry::Unconfigured("add"),
                &writes_candidates(),
            )
            .map(|_| ())
        });
        let by_add = by_add.unwrap_err();
        assert_eq!(by_add.error.code, code, "{specifier}: add: {by_add:?}");
        assert_eq!(files_under(existing.path()), before);

        let dir = scratch.path().join("fresh");
        let extensions = [specifier.to_string()];
        let request = init::Request {
            dir: &dir,
            name: Some("demo"),
            version: init::DEFAULT_VERSION,
            extensions: &extensions,
        };
        let by_init = init::init(&request, &writes_candidates()).unwrap_err();
        assert_eq!(by_init.error.code, code, "{specifier}: init: {by_init:?}");
        if code != "E063" {
            // A relative local path is read from the directory init creates, and named so.
            let path = dir.join(specifier);
            let want = by_add
                .error
                .message
                .replace(specifier, &path.to_string_lossy());
            assert_eq!(by_init.error.message, want, "{specifier}");
            assert_eq!(
                by_init.error.suggestion, by_add.error.suggestion,
                "{specifier}"
            );
        } else {
            assert!(
                by_init
                    .error
                    .suggestion
                    .as_deref()
                    .is_some_and(|s| s.contains("specforge add @acme/x")),
                "{by_init:?}"
            );
        }
        assert!(!dir.exists(), "{specifier}: nothing written");
    }
}
