// A locked extension loads only when its module declares what its lock entry records (ADR 0217).

use specforge_extension_sdk::{ContributionsBuilder, ExtensionMeta};
use specforge_installed::testing::install_module;
use specforge_installed::{read_lock_file, write_lock_file};
use specforge_protocol_types::PeerDependency;
use specforge_wasm::WasmRuntime;
use specforge_wasm::testing::InProcessRuntime;
use tempfile::TempDir;

use crate::load::{entries, load_with, reported};

const DEP: &[u8] = b"\0asm the binary of a";

fn peer(range: &str) -> PeerDependency {
    PeerDependency {
        name: "@acme/b".to_string(),
        version: range.to_string(),
        optional: false,
    }
}

/// `@acme/a` 1.0.0, requiring `@acme/b` ^1.0.
fn declaring() -> ContributionsBuilder {
    let mut meta = ExtensionMeta::new("@acme/a", "1.0.0");
    meta.peer_dependencies = vec![peer("^1.0")];
    ContributionsBuilder::new(meta)
}

/// Rewrite the lock entry of `@acme/a` to record `version` and `peers`.
fn record(root: &std::path::Path, version: &str, peers: Vec<PeerDependency>) {
    let path = specforge_installed::lock_path(root);
    let mut lock = read_lock_file(&path).unwrap();
    for entry in &mut lock.entries {
        entry.version = version.to_string();
        entry.peer_dependencies = peers.clone();
    }
    write_lock_file(&lock, &path).unwrap();
}

#[specforge_test_macros::test(
    behavior = "load_extension_manifests",
    verify = "an installed extension loads only when its binary declares the version and peers its specforge.lock entry records"
)]
#[ignore = "flipped by T4"]
fn a_module_declaring_another_version_or_other_peers_than_its_lock_entry_is_e070() {
    for (case, version, peers, refused) in [
        ("fewer peers", "1.0.0", vec![], true),
        ("another version", "7.7.7", vec![peer("^1.0")], true),
        ("another range", "1.0.0", vec![peer("^2.0")], true),
        ("what it declares", "1.0.0", vec![peer("^1.0")], false),
    ] {
        let dir = TempDir::new().unwrap();
        let runtime = InProcessRuntime::new().binary(DEP, declaring);
        install_module(dir.path(), &runtime, "@acme/a", DEP);
        record(dir.path(), version, peers);

        let loaded = load_with(dir.path(), &entries(&["@acme/a"]), &runtime);

        let reports = reported(dir.path(), &loaded);
        if refused {
            let codes: Vec<&str> = reports.iter().map(|d| d.code.as_str()).collect();
            assert_eq!(codes, ["E070"], "{case}: {reports:?}");
            assert!(
                reports[0].message.contains("its lock entry records"),
                "{case}: {reports:?}"
            );
            assert!(loaded.declarations.is_empty(), "{case}");
            assert!(
                !runtime.unload("@acme/a"),
                "{case}: nothing refused is left loaded"
            );
        } else {
            assert!(reports.is_empty(), "{case}: {reports:?}");
            assert_eq!(loaded.declarations.len(), 1, "{case}");
        }
    }
}
