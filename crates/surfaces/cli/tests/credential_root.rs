//! A registry credential belongs to the registry root it was kept for (ADR 0178): a project that names another
//! root under the same alias never gets it. The aliases are the same on purpose: a project picks its aliases, a
//! user does not.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use crate::support::mcp_process::{initialize, request, session};
use crate::support::registry::{project_on, serve};
use crate::support::{Cli, TestProject};
use specforge_test_macros::test as specforge_test;

/// A registry root that answers every request "no such package" and keeps each request's `Authorization` line.
struct Catcher {
    root: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Catcher {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = format!(
            "http://127.0.0.1:{}/b",
            listener.local_addr().unwrap().port()
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let line = head
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                    .unwrap_or("")
                    .to_string();
                log.lock().unwrap().push(line);
                let body = r#"{"error":{"code":"NOT_FOUND","message":"no such package"}}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Catcher { root, seen }
    }

    fn authorizations(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }

    /// A project whose only registry is this root, called `main`.
    fn project(&self) -> TestProject {
        TestProject::new()
            .enabling(&["@specforge/software"])
            .config(|c| {
                c["name"] = "p".into();
                c["registries"] = serde_json::json!([
                    { "alias": "main", "url": self.root, "default_registry": true }
                ]);
            })
    }
}

/// The user logs in to project A's registry (alias `main`) and the catcher is project B's registry (alias `main`).
fn logged_in_to_a_registry_called_main() -> (
    specforge_registry_server::testing::LocalRegistry,
    TestProject,
) {
    let registry = serve(vec![]);
    let project = project_on(&registry).config(|c| c["registries"][0]["alias"] = "main".into());
    let output = Cli::on(&project)
        .command(&[
            "login",
            "--registry",
            "main",
            "--token-env",
            "R20_P01_TOKEN",
        ])
        .env("R20_P01_TOKEN", registry.token())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (registry, project)
}

#[specforge_test(
    behavior = "authenticate_registry_request",
    verify = "a credential kept for one registry root is never sent to another root under the same alias"
)]
fn a_login_in_one_project_is_not_sent_to_another_projects_root() {
    let (registry, a) = logged_in_to_a_registry_called_main();
    let catcher = Catcher::start();
    let b = catcher.project();

    let _ = Cli::on(&b)
        .at_home(a.home())
        .command(&["add", "@acme/thing"])
        .env("R20_P01_TOKEN", registry.token())
        .output()
        .unwrap();

    let leaked = format!("Bearer {}", registry.token());
    let seen = catcher.authorizations();
    assert!(!seen.is_empty(), "the catcher saw no request");
    assert!(
        seen.iter().all(|line| !line.contains(&leaked)),
        "the token kept for another root was sent: {seen:?}"
    );
}

#[specforge_test(
    behavior = "provide_mcp_add_extension_tool",
    verify = "an add_extension dry run sends no credential kept for another registry root"
)]
fn an_add_extension_dry_run_sends_no_credential_kept_for_another_root() {
    let (registry, a) = logged_in_to_a_registry_called_main();
    let catcher = Catcher::start();
    let b = catcher.project();

    let _ = session(
        &b,
        b.root(),
        ".",
        &[
            initialize(1),
            request(
                2,
                "tools/call",
                serde_json::json!({
                    "name": "specforge.add_extension",
                    "arguments": {"specifiers": ["@acme/thing"], "dry_run": true}
                }),
            ),
        ],
        &[
            ("HOME", a.home().display().to_string()),
            ("R20_P01_TOKEN", registry.token().to_string()),
        ],
    );

    let leaked = format!("Bearer {}", registry.token());
    let seen = catcher.authorizations();
    assert!(!seen.is_empty(), "the catcher saw no request");
    assert!(
        seen.iter().all(|line| !line.contains(&leaked)),
        "the token kept for another root was sent: {seen:?}"
    );
}

#[specforge_test(
    behavior = "logout_registry",
    verify = "logout --url forgets the credential kept for that root without reading a project"
)]
fn logout_with_a_url_forgets_that_roots_credential_without_a_project() {
    let (registry, a) = logged_in_to_a_registry_called_main();
    let other = Catcher::start();
    let store = a.home().join(".specforge/credentials.json");
    // A second root's credential, kept beside the first.
    let mut kept: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&store).unwrap()).unwrap();
    kept["registries"][other.root.as_str()] = serde_json::json!({"token_env": "R20_P01_OTHER"});
    std::fs::write(&store, kept.to_string()).unwrap();

    // No project: the directory holds no specforge.json.
    let nowhere = TestProject::new().without_config();
    let output = Cli::on(&nowhere)
        .at_home(a.home())
        .bare_command(&[
            "logout",
            "--url",
            registry.root().as_str(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "removed", "{json}");
    assert_eq!(json["root"], registry.root().as_str(), "{json}");
    let left: serde_json::Value = serde_json::from_slice(&std::fs::read(&store).unwrap()).unwrap();
    let roots: Vec<&String> = left["registries"].as_object().unwrap().keys().collect();
    assert_eq!(roots, [&other.root]);
}
