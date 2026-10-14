//! A path an MCP request names resolves inside the served project (ADR 0092).
//!
//! Every test serves `proj/` and keeps an `outside/` directory beside it
//! with one unformatted spec file. A call that names `outside/` in any
//! spelling (`..`, absolute, through a symlink) is refused and leaves that
//! directory as it was.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// What `outside/x.spec` holds: `specforge format` rewrites it.
const UNFORMATTED: &str = "feature   b \"B\" {\n description    \"y\"\n}\n";

/// `proj/` (served) and `outside/` (not) side by side.
struct Scene {
    dir: TempDir,
}

impl Scene {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let proj = dir.path().join("proj");
        let outside = dir.path().join("outside");
        fs::create_dir_all(proj.join("spec")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(
            proj.join("specforge.json"),
            r#"{"name":"p","version":"0.1.0","extensions":["@specforge/software"]}"#,
        )
        .unwrap();
        fs::write(
            proj.join("spec/main.spec"),
            "behavior alpha \"Alpha\" {\n  contract \"MUST work\"\n}\n",
        )
        .unwrap();
        fs::write(outside.join("x.spec"), UNFORMATTED).unwrap();
        Scene { dir }
    }

    fn proj(&self) -> PathBuf {
        self.dir.path().join("proj")
    }

    fn outside(&self) -> PathBuf {
        self.dir.path().join("outside")
    }

    /// `outside/` as `new` made it: what an earlier call wrote is undone.
    fn restore_outside(&self) {
        let outside = self.outside();
        fs::remove_dir_all(&outside).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("x.spec"), UNFORMATTED).unwrap();
    }

    /// `specforge mcp .` run in `proj/`, initialized as a handshake client does.
    fn serve(&self) -> McpServer {
        let mut server = McpServer::with_project_root(self.proj());
        let reply = call(&mut server, "initialize", json!({}));
        assert!(reply["error"].is_null(), "{reply}");
        server
    }

    /// Every file under `outside/` with its bytes.
    fn outside_files(&self) -> Snapshot {
        files_under(&self.outside())
    }

    /// The spellings of `outside/` a client can send: relative to the
    /// project root, through `..` (absolute, so the process's directory does
    /// not matter), absolute, and through a symlink inside the project.
    fn spellings(&self) -> Vec<(&'static str, String)> {
        let proj = self.proj();
        let mut all = vec![
            ("relative", "../outside".to_string()),
            ("dotdot", format!("{}/../outside", proj.display())),
            ("absolute", self.outside().display().to_string()),
        ];
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(self.outside(), proj.join("link")).unwrap();
            all.push(("symlink", "link".to_string()));
            all.push(("absolute symlink", proj.join("link").display().to_string()));
        }
        all
    }
}

type Snapshot = crate::support::disk::Snapshot;

/// What went wrong with each spelling's call, `outside/` changed or the call
/// not refused as a containment violation: empty when every call held.
#[derive(Default)]
struct Verdicts(Vec<String>);

impl Verdicts {
    /// `reply` is the answer to naming `outside/` as `how`: nothing under it
    /// changed since `before`, and the call is refused on `argument`.
    fn of(&mut self, how: &str, scene: &Scene, before: &Snapshot, reply: &Value, argument: &str) {
        if &scene.outside_files() != before {
            self.0.push(format!("{how}: outside/ changed"));
        }
        let error = &reply["result"]["content"][0]["text"];
        let refused = reply["result"]["isError"] == true
            && error.as_str().is_some_and(|text| {
                serde_json::from_str::<Value>(text)
                    .is_ok_and(|e| e["code"] == "invalid_input" && e["argument"] == argument)
            });
        if !refused {
            self.0.push(format!(
                "{how}: not refused as invalid_input on {argument}: {reply}"
            ));
        }
    }

    fn assert_held(self) {
        assert!(self.0.is_empty(), "{}", self.0.join("\n"));
    }
}

/// The refusal a containment violation is: `invalid_input` on `argument`,
/// the code infer's source paths already refuse with.
fn assert_refused(reply: &Value, argument: &str) {
    let error = crate::tool_errors::mcp_error(reply);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], argument, "{error}");
}

/// The coordinator's repro: `specforge.format {"path": "../outside"}`.
#[test]
#[ignore = "fixed by T2"]
fn format_path_outside_the_served_project_is_refused() {
    let scene = Scene::new();
    let mut verdicts = Verdicts::default();
    for (how, path) in scene.spellings() {
        scene.restore_outside();
        let mut server = scene.serve();
        let before = scene.outside_files();
        let reply = call_tool(&mut server, "specforge.format", json!({ "path": path }));
        verdicts.of(how, &scene, &before, &reply, "path");
    }
    verdicts.assert_held();
}

/// The repro in the stateless revision (`specforge mcp .` serves per request).
#[test]
#[ignore = "fixed by T2"]
fn stateless_format_path_outside_the_served_project_is_refused() {
    let scene = Scene::new();
    let mut server = McpServer::with_project_root(scene.proj());
    let before = scene.outside_files();
    let params = json!({
        "name": "specforge.format",
        "arguments": {"path": format!("{}/../outside", scene.proj().display())},
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
            "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
        },
    });
    let reply = call(&mut server, "tools/call", params);
    let mut verdicts = Verdicts::default();
    verdicts.of("stateless", &scene, &before, &reply, "path");
    verdicts.assert_held();
}

/// The `paths` list joins each entry onto the project root: `..` and an
/// absolute entry leave it.
#[test]
#[ignore = "fixed by T2"]
fn format_paths_outside_the_served_project_are_refused() {
    let scene = Scene::new();
    let mut verdicts = Verdicts::default();
    for (how, path) in scene.spellings() {
        scene.restore_outside();
        let mut server = scene.serve();
        let before = scene.outside_files();
        let reply = call_tool(&mut server, "specforge.format", json!({ "paths": [path] }));
        verdicts.of(how, &scene, &before, &reply, "paths");
    }
    verdicts.assert_held();
}

/// Every tool that takes a `path` to another project (`path_tool_calls`),
/// named with `outside/`: refused, nothing read or written there.
#[test]
#[ignore = "fixed by T2"]
fn every_tool_with_a_path_refuses_one_outside_the_served_project() {
    let mut verdicts = Verdicts::default();
    for (tool, mut arguments) in path_tool_calls() {
        let scene = Scene::new();
        let mut server = scene.serve();
        let before = scene.outside_files();
        arguments["path"] = json!(format!("{}/../outside", scene.proj().display()));
        let reply = call_tool(&mut server, tool, arguments);
        verdicts.of(tool, &scene, &before, &reply, "path");
    }
    verdicts.assert_held();
}

/// A path inside the served project is still the served project. A relative
/// one is read against its root, not the process's directory (today it is
/// the latter, so `spec` is not found).
#[test]
#[ignore = "fixed by T2"]
fn a_path_inside_the_served_project_is_accepted() {
    let scene = Scene::new();
    let mut server = scene.serve();
    for path in [".", "spec", "../proj"] {
        let reply = call_tool(
            &mut server,
            "specforge.format",
            json!({"path": path, "check": true}),
        );
        assert_eq!(reply["result"]["isError"], false, "{path}: {reply}");
    }
}

/// `out_dir` is absolute as given or relative to the root: either leaves it.
#[test]
#[ignore = "fixed by T3"]
fn render_out_dir_outside_the_served_project_is_refused() {
    let scene = Scene::new();
    let mut verdicts = Verdicts::default();
    for (how, out_dir) in scene.spellings() {
        scene.restore_outside();
        let mut server = scene.serve();
        let before = scene.outside_files();
        let reply = call_tool(
            &mut server,
            "specforge.render",
            json!({"format": "graph", "out_dir": out_dir}),
        );
        verdicts.of(how, &scene, &before, &reply, "out_dir");
    }
    verdicts.assert_held();
}

/// `init` creates and writes a project at the `path` it is given.
#[test]
#[ignore = "fixed by T4"]
fn init_outside_the_served_project_is_refused() {
    let scene = Scene::new();
    let mut server = scene.serve();
    let target = scene.dir.path().join("created");
    let reply = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": target.display().to_string()}),
    );
    assert!(!target.exists(), "init created {}", target.display());
    assert_refused(&reply, "path");
}

/// `outline`'s `file` is joined onto the spec root: whether a file outside
/// it exists is answered (success against `file_not_found`).
#[test]
#[ignore = "fixed by T5"]
fn outline_of_a_file_outside_the_spec_root_is_refused() {
    let scene = Scene::new();
    let mut server = scene.serve();
    let reply = call_tool(
        &mut server,
        "specforge.outline",
        json!({"file": "../../outside/x.spec"}),
    );
    assert_refused(&reply, "file");
}

/// The precedent: infer's source paths are lexically contained already.
#[test]
fn infer_session_source_paths_outside_the_project_are_refused() {
    let scene = Scene::new();
    let mut server = scene.serve();
    let started = call_tool(
        &mut server,
        "specforge.infer_session",
        json!({"action": "start", "agent": "a", "source_roots": ["../outside"]}),
    );
    assert_refused(&started, "source_roots");
    let marked = call_tool(
        &mut server,
        "specforge.infer_session",
        json!({"action": "mark_analyzed", "source_file": "../outside/x.spec"}),
    );
    assert_refused(&marked, "source_file");
}

/// A source file or directory inside the project that is a symlink to
/// `outside/`: `format` with no paths walks the project's sources.
#[cfg(unix)]
#[test]
#[ignore = "fixed by T3"]
fn format_does_not_write_through_a_symlink_that_leaves_the_project() {
    let scene = Scene::new();
    std::os::unix::fs::symlink(scene.outside(), scene.proj().join("spec/dir")).unwrap();
    std::os::unix::fs::symlink(
        scene.outside().join("x.spec"),
        scene.proj().join("spec/file.spec"),
    )
    .unwrap();
    let mut server = scene.serve();
    let before = scene.outside_files();
    let reply = call_tool(&mut server, "specforge.format", json!({}));
    assert_eq!(scene.outside_files(), before, "outside/ changed: {reply}");
}

/// `mark_analyzed` reads the named file to hash it: through a symlink that
/// leaves the project, a file outside it.
#[cfg(unix)]
#[test]
#[ignore = "fixed by T4"]
fn infer_session_marks_no_file_behind_a_symlink_that_leaves_the_project() {
    let scene = Scene::new();
    std::os::unix::fs::symlink(scene.outside(), scene.proj().join("src")).unwrap();
    let mut server = scene.serve();
    let reply = call_tool(
        &mut server,
        "specforge.infer_session",
        json!({"action": "mark_analyzed", "source_file": "src/x.spec"}),
    );
    assert_refused(&reply, "source_file");
}
