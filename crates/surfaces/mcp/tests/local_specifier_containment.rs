//! A local extension an MCP call installs lies inside the project (ADR 0092 D7, closed by ADR 0098).
//!
//! Each test serves `proj/` and keeps a vendored blob in `outside/` beside it. A call that names that
//! blob, in any spelling, is refused as `invalid_input` and installs nothing.

use crate::support::*;
use serde_json::{Value, json};
use specforge_mcp::McpServer;
use specforge_test::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

struct Scene {
    dir: TempDir,
}

impl Scene {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let proj = dir.path().join("proj");
        fs::create_dir_all(proj.join("spec")).unwrap();
        fs::create_dir_all(dir.path().join("outside")).unwrap();
        fs::write(
            proj.join("specforge.json"),
            r#"{"name":"p","version":"0.1.0","extensions":[]}"#,
        )
        .unwrap();
        fs::copy(
            specforge_component::committed::GREET.path(),
            dir.path().join("outside/greet.wasm"),
        )
        .unwrap();
        Scene { dir }
    }

    fn outside_blob(&self) -> PathBuf {
        self.dir.path().join("outside/greet.wasm")
    }

    fn serve(&self) -> McpServer {
        let mut server = McpServer::with_project_root(self.dir.path().join("proj"));
        let reply = call(&mut server, "initialize", json!({}));
        assert!(reply["error"].is_null(), "{reply}");
        server
    }

    fn installed(&self) -> bool {
        self.dir.path().join("proj/specforge.lock").exists()
            || self.dir.path().join("proj/.specforge/extensions").exists()
    }
}

fn assert_refused(reply: &Value, argument: &str) {
    let error = crate::tool_errors::mcp_error(reply);
    assert_eq!(error["code"], "invalid_input", "{error}");
    assert_eq!(error["argument"], argument, "{error}");
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "a local extension path in specifiers that resolves outside the project is refused as invalid_input on specifiers"
)]
fn add_extension_refuses_a_local_file_outside_the_project() {
    let scene = Scene::new();
    let mut server = scene.serve();
    let spellings = [
        scene.outside_blob().display().to_string(),
        "../outside/greet.wasm".to_string(),
    ];
    for specifier in spellings {
        let reply = call_tool(
            &mut server,
            "specforge.add_extension",
            json!({"specifiers": [specifier]}),
        );
        assert_refused(&reply, "specifiers");
        assert!(!scene.installed(), "installed from {specifier}");
    }
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "a local extension path in specifiers that resolves outside the project is refused as invalid_input on specifiers"
)]
fn add_extension_refuses_a_symlink_that_leaves_the_project() {
    let scene = Scene::new();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            scene.outside_blob(),
            scene.dir.path().join("proj/greet.wasm"),
        )
        .unwrap();
        let mut server = scene.serve();
        let reply = call_tool(
            &mut server,
            "specforge.add_extension",
            json!({"specifiers": ["greet.wasm"]}),
        );
        assert_refused(&reply, "specifiers");
        assert!(!scene.installed());
    }
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "a local extension path in extensions that resolves outside the served project is refused as invalid_input on extensions"
)]
fn init_refuses_a_local_file_outside_the_servers_root() {
    // Nothing is served, so the server's root bounds the call.
    let scene = Scene::new();
    let root = scene.dir.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    let mut server = McpServer::with_project_root(root.clone());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    let reply = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": "created", "extensions": [scene.outside_blob().display().to_string()]}),
    );
    assert_refused(&reply, "extensions");
    assert!(!root.join("created").exists());
}

/// A server rooted at `dir/workspace`, nothing served: the root bounds `init`.
fn workspace_server(scene: &Scene) -> (McpServer, PathBuf) {
    let root = scene.dir.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    let mut server = McpServer::with_project_root(root.clone());
    let reply = call(&mut server, "initialize", json!({}));
    assert!(reply["error"].is_null(), "{reply}");
    (server, root)
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "each local specifier in extensions is held to the server's root as init reads it, a crate directory included"
)]
fn init_refuses_a_local_file_named_after_a_comma() {
    let scene = Scene::new();
    let (mut server, root) = workspace_server(&scene);
    let reply = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": "created", "extensions": ["@specforge/software,../../outside/greet.wasm"]}),
    );
    // One string is one specifier (ADR 0105): the whole string is a local path that names nothing,
    // so the blob outside is never reached.
    let error = crate::tool_errors::mcp_error(&reply);
    assert_eq!(error["code"], "file_not_found", "{error}");
    assert_eq!(error["argument"], "extensions", "{error}");
    assert!(!root.join("created").exists());
}

#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "each local specifier in extensions is held to the server's root as init reads it, a crate directory included"
)]
fn init_refuses_a_crate_directory_named_after_a_comma() {
    let scene = Scene::new();
    let ext = scene.dir.path().join("outside/ext");
    fs::create_dir_all(ext.join("target/wasm32-wasip2/release")).unwrap();
    fs::write(ext.join("Cargo.toml"), "[package]\nname = \"greet\"\n").unwrap();
    fs::copy(
        scene.outside_blob(),
        ext.join("target/wasm32-wasip2/release/greet.wasm"),
    )
    .unwrap();
    let (mut server, root) = workspace_server(&scene);
    let reply = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": "created", "extensions": ["@specforge/software,../../outside/ext"]}),
    );
    assert_refused(&reply, "extensions");
    assert!(!root.join("created").exists());
}

#[cfg(unix)]
#[specforge_test(
    behavior = "provide_mcp_init_tool",
    verify = "each local specifier in extensions is held to the server's root as init reads it, a crate directory included"
)]
fn init_refuses_a_local_file_reached_through_a_symlink_and_dot_dots() {
    let scene = Scene::new();
    fs::copy(scene.outside_blob(), scene.dir.path().join("greet.wasm")).unwrap();
    let (mut server, root) = workspace_server(&scene);
    let deep = root.join("existing/d1/d2/d3");
    fs::create_dir_all(&deep).unwrap();
    std::os::unix::fs::symlink(&deep, root.join("existing/a")).unwrap();
    let reply = call_tool(
        &mut server,
        "specforge.init",
        json!({"path": "existing", "extensions": ["a/../../../greet.wasm"]}),
    );
    // After T3 the checked path is ops' lexical one (`invalid_input`); after T6 init reads what
    // the OS reads, `existing/greet.wasm`, which is missing (`file_not_found`): either way an
    // error on `extensions`, and nothing is installed from outside.
    let error = crate::tool_errors::mcp_error(&reply);
    assert_eq!(error["argument"], "extensions", "{error}");
    assert!(!root.join("existing/specforge.json").exists());
    let lock = fs::read_to_string(root.join("existing/specforge.lock")).unwrap_or_default();
    assert!(!lock.contains("greet.wasm"), "{lock}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "a crate directory in specifiers whose built component resolves outside the project is refused as invalid_input on specifiers"
)]
fn add_extension_refuses_a_crate_directory_whose_build_leaves_the_project() {
    let scene = Scene::new();
    let proj = scene.dir.path().join("proj");
    let built = scene
        .dir
        .path()
        .join("outside/target/wasm32-wasip2/release");
    fs::create_dir_all(&built).unwrap();
    fs::copy(scene.outside_blob(), built.join("greet.wasm")).unwrap();
    fs::create_dir_all(proj.join("ext")).unwrap();
    fs::write(proj.join("ext/Cargo.toml"), "[package]\nname = \"greet\"\n").unwrap();
    std::os::unix::fs::symlink(
        scene.dir.path().join("outside/target"),
        proj.join("ext/target"),
    )
    .unwrap();
    let mut server = scene.serve();
    let reply = call_tool(
        &mut server,
        "specforge.add_extension",
        json!({"specifiers": ["ext"]}),
    );
    assert_refused(&reply, "specifiers");
    assert!(!scene.installed());
}
