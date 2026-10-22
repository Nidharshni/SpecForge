//! A dry run decides a registry package's publisher as the add does, before any download (ADR 0213).

use crate::support::registry::{Package, greet_wasm, project_on, serve};
use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

/// `add @sdk/greet@0.1.0 --format json`, with `extra` arguments: its exit status and parsed stdout.
fn add_greet(dir: &TestProject, extra: &[&str]) -> (bool, serde_json::Value) {
    let mut args = vec!["add", "@sdk/greet@0.1.0", "--format", "json"];
    args.extend(extra);
    let output = Cli::on(dir).command(&args).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    (output.status.success(), json)
}

#[specforge_test(
    invariant = "publisher_trust",
    verify = "specforge add --dry-run refuses an unsigned package and a denied publisher key as specforge add does, downloading nothing"
)]
#[ignore = "flipped by T4"]
fn a_dry_run_refuses_what_the_add_refuses_and_downloads_nothing() {
    let denied = specforge_registry_client::SigningKey::generate();
    let cases = [(None, "R-TRUST-001"), (Some(denied), "R-TRUST-005")];
    for (signer, code) in cases {
        let package = Package::new("@sdk/greet", "0.1.0", greet_wasm());
        let package = match &signer {
            Some(key) => package.signed_by(key),
            None => package,
        };
        let registry = serve(vec![package]);
        let dir = project_on(&registry);
        if let Some(key) = &signer {
            let mut known = specforge_registry_client::KnownKeys::default();
            known.denied_keys.push(key.key_id());
            specforge_registry_client::save_known_keys_at(
                &crate::registry::known_keys(dir.home()),
                &known,
            )
            .unwrap();
        }

        let (dry_ok, dry) = add_greet(&dir, &["--dry-run"]);
        let (add_ok, real) = add_greet(&dir, &[]);

        assert!(!dry_ok && !add_ok, "{code}: {dry} / {real}");
        assert_eq!((&dry["code"], &real["code"]), (&code.into(), &code.into()));
        assert!(!dir.root().join("specforge.lock").exists());
        let downloads: Vec<_> = registry
            .requests()
            .into_iter()
            .filter(|r| r.ends_with("/download"))
            .collect();
        assert!(
            downloads.is_empty(),
            "a refused package is not downloaded: {downloads:?}"
        );
    }
}
