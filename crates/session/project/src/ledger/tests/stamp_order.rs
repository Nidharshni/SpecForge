//! The ledger stamps a file in the call that reads it: a write after the stamp is seen next time
//! (behavior `bring_session_up_to_date`).

use std::fs;
use std::path::Path;

use specforge_common::ProjectRoot;

use std::sync::Arc;

use specforge_wasm::testing::InProcessRuntime;

use crate::ledger::recorded::Recording;
use crate::ledger::{Ledger, Reading, Reads};
use crate::sources::Read;
use crate::{Environment, SharedRuntime};

const CONFIG: &str = "{\"name\":\"d\",\"version\":\"0.1.0\",\"extensions\":[]}\n";

fn project(config: &str) -> (tempfile::TempDir, ProjectRoot) {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("specforge.json"), config).unwrap();
    let root = ProjectRoot::of(dir.path()).unwrap();
    (dir, root)
}

/// Rewrite `path` so that its stamp (size) differs.
fn rewrite(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn a_config_written_after_its_read_is_stale() {
    let (dir, root) = project(CONFIG);
    let mut ledger = Ledger::opening();
    Environment::read_through(&mut ledger, &root, |_| None);
    assert!(ledger.stale().is_empty());
    rewrite(
        &dir.path().join("specforge.json"),
        &format!("{CONFIG}\n\n// written after the read\n"),
    );
    assert_eq!(
        ledger.stale().environment,
        [dir.path().join("specforge.json")]
    );
}

#[test]
fn a_module_written_after_its_read_is_stale() {
    let (dir, root) = project(
        "{\"name\":\"d\",\"version\":\"0.1.0\",\"extensions\":[\"@acme/local=ext/local.wasm\"]}\n",
    );
    let module = dir.path().join("ext/local.wasm");
    rewrite(&module, "first");
    let mut ledger = Ledger::opening();
    // The extension load runs (a runtime is given), so it reads the module.
    Environment::read_through(&mut ledger, &root, |_| {
        Some(Arc::new(InProcessRuntime::new()) as SharedRuntime)
    });
    assert!(ledger.stale().is_empty());
    rewrite(&module, "second, longer");
    assert_eq!(ledger.stale().environment, [module]);
}

#[test]
fn a_source_that_is_no_source_any_more_leaves_the_record() {
    let (dir, root) = project(CONFIG);
    rewrite(&dir.path().join("sub/c.spec"), "behavior c \"C\" {\n}\n");
    let mut ledger = Ledger::opening();
    let env = Environment::read_through(&mut ledger, &root, |_| None);
    let sources = env.config().sources(root.dir());
    assert_eq!(ledger.discover(&sources), ["sub/c.spec"]);
    ledger.source(sources.dir(), "sub/c.spec");
    assert!(ledger.stale().is_empty());

    // A nested project took it.
    let marker = dir.path().join("sub/specforge.json");
    rewrite(&marker, CONFIG);
    let reads = ledger.reread(&["sub/c.spec".to_string()]);
    assert!(matches!(reads[0].1, Read::Gone));
    assert!(ledger.stale().sources.is_empty());

    // The marker goes: its return is seen.
    fs::remove_file(&marker).unwrap();
    assert_eq!(ledger.stale().sources, ["sub/c.spec"]);
}

#[test]
fn a_detached_ledger_reads_every_key_as_gone_and_records_nothing() {
    let mut ledger = Ledger::detached();
    let reads = ledger.reread(&["/p/a.spec".to_string()]);
    assert!(matches!(reads[0].1, Read::Gone));
    assert!(ledger.stale().is_empty());
    assert!(ledger.changes([Path::new("/p/a.spec")]).is_empty());
    let run = ledger.checks();
    assert!(run.read(Reading::File, Path::new("/p/x.md"), |_| true));
    run.finish(Recording::Checks);
    assert_eq!(ledger.inputs(), &crate::ledger::SessionInputs::detached());
}
