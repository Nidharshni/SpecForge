//! What `specforge.render` does with the file it writes into `out_dir`: the defects of its own write.
//! Each pin is ignored until the ticket that closes the defect flips it.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// `root` served, initialized as a handshake client does.
fn serve(root: &Path) -> McpServer {
    let mut server = McpServer::with_project_root(root.to_path_buf());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    server
}

/// A project with one source file and the software extension.
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

/// The refusal `reply` is, as `(code, argument)`.
fn refusal(reply: &Value) -> (String, Option<String>) {
    let error = crate::tool_errors::mcp_error(reply);
    (
        error["code"].as_str().unwrap().to_string(),
        error["argument"].as_str().map(str::to_string),
    )
}

/// A file outside the project that is a hard link at the output's name is not written through.
#[cfg(unix)]
#[test]
#[ignore = "flipped by T5"]
fn render_writes_no_file_through_a_hard_link_to_a_file_outside() {
    let project = plain_project();
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("victim.json");
    fs::write(&victim, "ORIGINAL").unwrap();
    fs::create_dir_all(project.path().join("hard")).unwrap();
    fs::hard_link(&victim, project.path().join("hard/brief.json")).unwrap();
    let mut server = serve(project.path());

    let reply = tool(
        &mut server,
        "specforge.render",
        json!({"format": "brief", "out_dir": "hard"}),
    );

    assert_eq!(reply["output_files"], json!(["hard/brief.json"]), "{reply}");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    let written: Value =
        serde_json::from_str(&fs::read_to_string(project.path().join("hard/brief.json")).unwrap())
            .unwrap();
    assert_eq!(written["nodes"][0]["id"], "alpha", "{written}");
}

/// A directory the user cannot write is `permission_denied`, and nothing is left in it.
#[cfg(unix)]
#[test]
#[ignore = "flipped by T5"]
fn render_into_a_directory_it_cannot_write_is_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let project = plain_project();
    let rodir = project.path().join("rodir");
    fs::create_dir_all(&rodir).unwrap();
    fs::set_permissions(&rodir, fs::Permissions::from_mode(0o555)).unwrap();
    // A process that may write there anyway (root) cannot show the refusal.
    let probe = rodir.join("probe");
    if fs::write(&probe, "x").is_ok() {
        fs::remove_file(&probe).unwrap();
        fs::set_permissions(&rodir, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let mut server = serve(project.path());

    let reply = call_tool(
        &mut server,
        "specforge.render",
        json!({"format": "brief", "out_dir": "rodir"}),
    );

    fs::set_permissions(&rodir, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        refusal(&reply),
        ("permission_denied".to_string(), Some("out_dir".to_string())),
        "{reply}"
    );
    assert!(!rodir.join("brief.json").exists());
    assert!(!rodir.join("brief.json.tmp").exists());
}

/// An `out_dir` that is, or goes through, a file is invalid input about `out_dir`.
#[test]
#[ignore = "flipped by T5"]
fn render_into_an_out_dir_that_is_a_file_is_invalid_input_on_out_dir() {
    let project = plain_project();
    let source = project.path().join("spec/main.spec");
    let before = fs::read_to_string(&source).unwrap();
    let mut server = serve(project.path());

    for out_dir in ["spec/main.spec", "spec/main.spec/sub"] {
        let reply = call_tool(
            &mut server,
            "specforge.render",
            json!({"format": "brief", "out_dir": out_dir}),
        );
        assert_eq!(
            refusal(&reply),
            ("invalid_input".to_string(), Some("out_dir".to_string())),
            "{out_dir}: {reply}"
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), before, "{out_dir}");
    }
}

/// A directory reached through a missing one and back is named from the project root.
#[test]
fn render_names_its_file_from_the_root_through_a_missing_directory() {
    let project = plain_project();
    let mut server = serve(project.path());

    let reply = tool(
        &mut server,
        "specforge.render",
        json!({"format": "brief", "out_dir": "nope/../out/y"}),
    );

    assert_eq!(
        reply["output_files"],
        json!(["out/y/brief.json"]),
        "{reply}"
    );
    assert!(project.path().join("out/y/brief.json").exists());
}

/// A link that stays inside the project is followed, and named as spelled.
#[cfg(unix)]
#[test]
fn render_follows_a_link_that_stays_inside_and_names_the_link() {
    let project = plain_project();
    fs::create_dir_all(project.path().join("out")).unwrap();
    std::os::unix::fs::symlink(project.path().join("out"), project.path().join("linkin")).unwrap();
    let mut server = serve(project.path());

    let reply = tool(
        &mut server,
        "specforge.render",
        json!({"format": "brief", "out_dir": "linkin"}),
    );

    assert_eq!(
        reply["output_files"],
        json!(["linkin/brief.json"]),
        "{reply}"
    );
    assert!(project.path().join("out/brief.json").exists());
    assert!(
        fs::symlink_metadata(project.path().join("linkin"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
