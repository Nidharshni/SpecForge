//! A hard link planted inside the project is never written through (ADR 0144). A path rule cannot see a
//! hard link, so the rule is on the write: a project file is replaced by its name, never opened in
//! place. Each pin is a name a writer owns, hard-linked to a file outside the project.
//! A pin marked ignored is flipped by the ticket that closes its writer.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn serve(root: &Path) -> McpServer {
    let mut server = McpServer::with_project_root(root.to_path_buf());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

fn plain_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("spec")).unwrap();
    fs::write(
        dir.path().join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("spec/main.spec"),
        "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n",
    )
    .unwrap();
    dir
}

/// A project with an old-format source, so `migrate` has something to write.
fn old_project() -> TempDir {
    let dir = plain_project();
    fs::write(
        dir.path().join("spec/main.spec"),
        "// specforge-format: 0.9\nbehavior alpha \"Alpha\" {\n}\n",
    )
    .unwrap();
    dir
}

/// An outside file holding `ORIGINAL`, and the path of a hard link to it at `at` in `project`.
fn plant(project: &Path, at: &str) -> (TempDir, std::path::PathBuf) {
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("victim.txt");
    fs::write(&victim, "ORIGINAL").unwrap();
    let name = project.join(at);
    fs::create_dir_all(name.parent().unwrap()).unwrap();
    fs::hard_link(&victim, &name).unwrap();
    (outside, victim)
}

fn untouched(victim: &Path) {
    assert_eq!(fs::read_to_string(victim).unwrap(), "ORIGINAL");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn migrate_writes_no_backup_or_temporary_file_through_a_hard_link() {
    for at in ["spec/main.spec.bak", "spec/main.spec.tmp"] {
        let project = old_project();
        let (_outside, victim) = plant(project.path(), at);
        let mut server = serve(project.path());

        let reply = tool(&mut server, "specforge.migrate", json!({}));

        assert_eq!(reply["ok"], true, "{at}: {reply}");
        untouched(&victim);
    }
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn format_replaces_a_hard_linked_source_and_keeps_the_other_name() {
    let project = plain_project();
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("victim.txt");
    fs::write(&victim, "behavior   alpha   \"Alpha\"   {\n}\n").unwrap();
    fs::remove_file(project.path().join("spec/main.spec")).unwrap();
    fs::hard_link(&victim, project.path().join("spec/main.spec")).unwrap();
    let mut server = serve(project.path());

    let _ = call_tool(&mut server, "specforge.format", json!({}));

    assert_eq!(
        fs::read_to_string(&victim).unwrap(),
        "behavior   alpha   \"Alpha\"   {\n}\n"
    );
    let mine = fs::read_to_string(project.path().join("spec/main.spec")).unwrap();
    assert!(mine.starts_with("behavior alpha \"Alpha\" {"), "{mine}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn rename_of_a_hard_linked_source_is_refused_and_writes_neither_name() {
    let project = plain_project();
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("victim.txt");
    let text = "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n";
    fs::write(&victim, text).unwrap();
    fs::remove_file(project.path().join("spec/main.spec")).unwrap();
    fs::hard_link(&victim, project.path().join("spec/main.spec")).unwrap();
    let mut server = serve(project.path());

    let reply = call_tool(
        &mut server,
        "specforge.rename",
        json!({"entity_id": "alpha", "new_name": "omega"}),
    );

    // The session does not read a source with two names (ADR 0146), so it has no `alpha` to rename:
    // the rename is refused and neither name is written.
    assert!(reply.to_string().contains("alpha"), "{reply}");
    assert_eq!(fs::read_to_string(&victim).unwrap(), text);
    let mine = fs::read_to_string(project.path().join("spec/main.spec")).unwrap();
    assert_eq!(mine, text);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn init_replaces_a_hard_linked_gitignore() {
    let project = TempDir::new().unwrap();
    let (_outside, victim) = plant(project.path(), ".gitignore");
    let mut server = serve(project.path());

    let _ = call_tool(&mut server, "specforge.init", json!({}));

    untouched(&victim);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn collect_replaces_a_hard_linked_report() {
    let project = crate::operations_mgmt::collect_project();
    let (_outside, victim) = plant(project.path(), "specforge-report.json");
    let mut server = serve(project.path());

    let _ = call_tool(&mut server, "specforge.collect", json!({}));

    untouched(&victim);
    let mine = fs::read_to_string(project.path().join("specforge-report.json")).unwrap();
    assert!(mine.contains("alpha"), "{mine}");
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn infer_session_replaces_a_hard_linked_manifest() {
    let project = plain_project();
    let (_outside, victim) = plant(project.path(), "specforge-infer.json");
    let mut server = serve(project.path());

    let _ = call_tool(
        &mut server,
        "specforge.infer_session",
        json!({"action": "start"}),
    );

    untouched(&victim);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn migrate_record_is_not_written_through_a_hard_link() {
    let project = old_project();
    let (_outside, victim) = plant(project.path(), ".specforge/migration.json");
    let mut server = serve(project.path());

    let _ = call_tool(&mut server, "specforge.migrate", json!({}));

    untouched(&victim);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a mutation writes no file through a hard link to a file outside the project"
)]
fn add_extension_writes_no_lock_through_a_hard_linked_or_symlinked_temp() {
    let greet = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/greet-extension/wasm/greet.wasm");
    for symlink in [false, true] {
        let project = plain_project();
        fs::copy(&greet, project.path().join("greet.wasm")).unwrap();
        let outside = TempDir::new().unwrap();
        let victim = outside.path().join("victim.txt");
        fs::write(&victim, "ORIGINAL").unwrap();
        let temp = project.path().join("specforge.lock.tmp");
        if symlink {
            std::os::unix::fs::symlink(&victim, &temp).unwrap();
        } else {
            fs::hard_link(&victim, &temp).unwrap();
        }
        let mut server = serve(project.path());

        let reply: Value = call_tool(
            &mut server,
            "specforge.add_extension",
            json!({"specifiers": ["./greet.wasm"]}),
        );

        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            "ORIGINAL",
            "symlink={symlink}: {reply}"
        );
    }
}
