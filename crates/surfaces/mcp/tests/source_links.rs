//! A source of the served project is read only from inside the project (ADR 0145). Over MCP the session
//! reads each `.spec` source through the project's containment: a link whose final target lands outside
//! is that source's E025 and its text enters no view.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const SECRET: &str = "SECRETWORD";

fn serve(root: &Path) -> McpServer {
    let mut server = McpServer::with_project_root(root.to_path_buf());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

/// A project with one source and no extension, and a directory beside it that is outside.
fn scene() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let scene = TempDir::new().unwrap();
    let project = scene.path().join("proj");
    let outside = scene.path().join("outside");
    fs::create_dir_all(project.join("spec")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        project.join("specforge.json"),
        r#"{"name":"p","version":"0.1.0","spec_root":"spec","extensions":[]}"#,
    )
    .unwrap();
    fs::write(
        project.join("spec/a.spec"),
        "behavior alpha \"Alpha\" { }\n",
    )
    .unwrap();
    (scene, project, outside)
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).unwrap();
}

/// Every reply of the views that read the graph or its diagnostics, as one string.
fn views(server: &mut McpServer) -> String {
    let mut all = String::new();
    for (name, arguments) in [
        ("specforge.validate", json!({})),
        ("specforge.list", json!({})),
        ("specforge.search", json!({"query": "leaked"})),
        ("specforge.export", json!({})),
    ] {
        all += &call_tool(server, name, arguments).to_string();
    }
    for uri in [
        "specforge://diagnostics",
        "specforge://graph",
        "specforge://brief",
    ] {
        all += &call(server, "resources/read", json!({"uri": uri})).to_string();
    }
    all
}

fn assert_nothing_of(views: &str, needles: &[&str]) {
    for needle in needles {
        assert!(!views.contains(needle), "{needle} reached a view: {views}");
    }
}

/// The text a spec outside the project holds: an entity and a field value.
const OUTSIDE_SPEC: &str =
    "behavior leaked \"Leaked Behavior\" {\n  summary \"outside-body-text\"\n}\n";

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_source_symlink_that_leaves_the_project_is_read_by_no_view() {
    let (_scene, project, outside) = scene();
    fs::write(
        outside.join("secret.txt"),
        format!("{SECRET} not a valid spec\nline-two-password=hunter2\n"),
    )
    .unwrap();
    link(&outside.join("secret.txt"), &project.join("spec/leak.spec"));
    let mut server = serve(&project);
    let all = views(&mut server);
    assert!(all.contains("E025"), "{all}");
    assert_nothing_of(&all, &[SECRET, "hunter2", "password"]);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_source_symlink_to_a_spec_outside_enters_no_graph() {
    let (_scene, project, outside) = scene();
    fs::write(outside.join("leaked.spec"), OUTSIDE_SPEC).unwrap();
    link(
        &outside.join("leaked.spec"),
        &project.join("spec/leak.spec"),
    );
    let mut server = serve(&project);
    let all = views(&mut server);
    assert!(all.contains("E025"), "{all}");
    assert_nothing_of(&all, &["Leaked Behavior", "outside-body-text"]);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_symlinked_spec_root_that_leaves_the_project_enters_no_graph() {
    let (_scene, project, outside) = scene();
    fs::create_dir_all(outside.join("specdir")).unwrap();
    fs::write(outside.join("specdir/leaked.spec"), OUTSIDE_SPEC).unwrap();
    fs::remove_dir_all(project.join("spec")).unwrap();
    link(&outside.join("specdir"), &project.join("spec"));
    let mut server = serve(&project);
    let all = views(&mut server);
    assert_nothing_of(&all, &["Leaked Behavior", "outside-body-text"]);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_link_retargeted_outside_after_the_session_opened_is_refused_at_the_next_request() {
    let (_scene, project, outside) = scene();
    fs::write(outside.join("leaked.spec"), OUTSIDE_SPEC).unwrap();
    fs::write(
        project.join("spec/real.spec"),
        "behavior inner \"Inner\" { }\n",
    )
    .unwrap();
    link(
        &project.join("spec/real.spec"),
        &project.join("spec/l.spec"),
    );
    let mut server = serve(&project);
    let before = views(&mut server);
    assert!(before.contains("inner"), "{before}");

    fs::remove_file(project.join("spec/l.spec")).unwrap();
    link(&outside.join("leaked.spec"), &project.join("spec/l.spec"));
    let after = views(&mut server);
    assert!(after.contains("E025"), "{after}");
    assert_nothing_of(&after, &["Leaked Behavior", "outside-body-text"]);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn another_project_a_call_names_reads_no_source_through_a_link_outside_it() {
    let (_scene, project, outside) = scene();
    let other = project.join("nested");
    fs::create_dir_all(other.join("spec")).unwrap();
    fs::write(
        other.join("specforge.json"),
        r#"{"name":"n","version":"0.1.0","spec_root":"spec","extensions":[]}"#,
    )
    .unwrap();
    fs::write(
        outside.join("secret.txt"),
        format!("{SECRET} not a valid spec\n"),
    )
    .unwrap();
    link(&outside.join("secret.txt"), &other.join("spec/leak.spec"));
    let mut server = serve(&project);
    let reply = call_tool(
        &mut server,
        "specforge.validate",
        json!({"path": other.to_str().unwrap()}),
    )
    .to_string();
    assert!(reply.contains("E025"), "{reply}");
    assert_nothing_of(&reply, &[SECRET]);
}

/// A chain that stays inside the project, and a hop outside whose final target is inside, keep working.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_source_link_whose_final_target_is_inside_the_project_is_read() {
    let (_scene, project, outside) = scene();
    fs::create_dir_all(project.join("real")).unwrap();
    fs::create_dir_all(project.join("links")).unwrap();
    fs::write(
        project.join("real/b.spec"),
        "behavior inner \"Inner\" { }\n",
    )
    .unwrap();
    link(&project.join("real/b.spec"), &project.join("links/l1"));
    link(&project.join("links/l1"), &project.join("spec/a2.spec"));
    link(&project.join("real/b.spec"), &outside.join("hop"));
    link(&outside.join("hop"), &project.join("spec/h.spec"));
    let mut server = serve(&project);
    let reply = call_tool(&mut server, "specforge.list", json!({})).to_string();
    assert!(reply.contains("inner"), "{reply}");
    assert!(!reply.contains("E025"), "{reply}");
}

/// A file with more than one name is another name for a file outside: no view reads it (ADR 0146).
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_hard_linked_source_that_is_not_a_spec_is_read_by_no_view() {
    let (_scene, project, outside) = scene();
    fs::write(
        outside.join("secret.txt"),
        format!("{SECRET} not a valid spec\nline-two-password=hunter2\n"),
    )
    .unwrap();
    fs::hard_link(outside.join("secret.txt"), project.join("spec/leak.spec")).unwrap();
    let mut server = serve(&project);
    let all = views(&mut server);
    assert!(all.contains("E025"), "{all}");
    assert_nothing_of(&all, &[SECRET, "hunter2", "password"]);
}

#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_hard_linked_source_that_is_a_valid_spec_enters_no_graph() {
    let (_scene, project, outside) = scene();
    fs::write(outside.join("leaked.spec"), OUTSIDE_SPEC).unwrap();
    fs::hard_link(outside.join("leaked.spec"), project.join("spec/leak.spec")).unwrap();
    let mut server = serve(&project);
    let all = views(&mut server);
    assert!(all.contains("E025"), "{all}");
    assert_nothing_of(
        &all,
        &["Leaked Behavior", "outside-body-text", "\"leaked\""],
    );
}

/// A hard-linked recorded report steers no coverage verdict.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_hard_linked_recorded_report_steers_no_coverage_verdict() {
    let (_scene, project, outside) = scene();
    fs::write(
        outside.join("r.json"),
        r#"{"results":{"alpha":{"tests":[{"name":"OUTSIDE-TEST","status":"passed"}]}}}"#,
    )
    .unwrap();
    fs::hard_link(
        outside.join("r.json"),
        project.join("specforge-report.json"),
    )
    .unwrap();
    let mut server = serve(&project);
    for name in ["specforge.coverage", "specforge.stats"] {
        let reply = call_tool(&mut server, name, json!({})).to_string();
        assert!(reply.contains("E045"), "{name}: {reply}");
        assert_nothing_of(&reply, &["OUTSIDE-TEST"]);
    }
}

/// A plain file, and a symlink that stays inside, still read: only a second name is refused.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_source_with_one_name_is_read() {
    let (_scene, project, _outside) = scene();
    fs::write(
        project.join("spec/p.spec"),
        "behavior plain \"Plain\" { }\n",
    )
    .unwrap();
    let mut server = serve(&project);
    let reply = call_tool(&mut server, "specforge.list", json!({})).to_string();
    assert!(reply.contains("plain"), "{reply}");
    assert!(!reply.contains("E025"), "{reply}");
}

/// The refusal is on the count, so a second name that is also inside the project is refused too (ADR 0146 D3).
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_cp_al_snapshot_served_directly_is_refused() {
    let (_scene, project, _outside) = scene();
    fs::hard_link(project.join("spec/a.spec"), project.join("spec/copy.spec")).unwrap();
    let mut server = serve(&project);
    let reply = call_tool(&mut server, "specforge.validate", json!({})).to_string();
    assert!(reply.contains("E025"), "{reply}");
    assert!(reply.contains("2 names"), "{reply}");
}

/// A writer replaces the name (ADR 0144), so a rewritten source has one name and reads again; the
/// format's own read of the project's state before the rewrite is not bounded by the link count.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_source_rewritten_by_format_has_one_name_and_reads() {
    let (_scene, project, outside) = scene();
    fs::write(
        outside.join("leaked.spec"),
        "behavior   leaked   \"Leaked\"   {\n}\n",
    )
    .unwrap();
    fs::hard_link(outside.join("leaked.spec"), project.join("spec/leak.spec")).unwrap();
    let mut server = serve(&project);
    let reply = call_tool(&mut server, "specforge.format", json!({"write": true})).to_string();
    let after = call_tool(&mut server, "specforge.list", json!({})).to_string();
    assert!(reply.contains("spec/leak.spec"), "{reply}");
    assert!(after.contains("leaked"), "{after}");
    assert!(!after.contains("E025"), "{after}");
}

/// A symlinked directory inside the spec root is not entered at all.
#[cfg(unix)]
#[specforge_test_macros::test(
    invariant = "mcp_served_project_consistency",
    verify = "a source of the served project is read only from inside the project"
)]
fn a_symlinked_directory_inside_the_spec_root_is_not_entered() {
    let (_scene, project, outside) = scene();
    fs::create_dir_all(outside.join("dir")).unwrap();
    fs::write(outside.join("dir/x.spec"), OUTSIDE_SPEC).unwrap();
    link(&outside.join("dir"), &project.join("spec/sub"));
    let mut server = serve(&project);
    let all = views(&mut server);
    assert_nothing_of(&all, &["Leaked Behavior"]);
}

#[allow(dead_code, reason = "keeps Value in scope for non-unix builds")]
fn _unused(_: Value) {}
