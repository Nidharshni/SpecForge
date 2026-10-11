//! H3/H5 gate (hardening-plan): `specforge watch --json` must react to a
//! `specforge.json` edit (extension list change) with an
//! `extensions_reloaded` event — no restart required (R-5).

use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

use crate::child_guard::{ChildGuard, guarded_command};

#[test]
fn watch_reloads_extension_environment_on_config_change() {
    let dir = TempDir::new().unwrap();
    let config = serde_json::json!({
        "name": "watch-reload",
        "version": "0.1.0",
        "extensions": ["@specforge/product"],
    });
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();
    fs::write(
        dir.path().join("main.spec"),
        r#"feature watched "Watched feature" {
    status proposed
    priority high
}
"#,
    )
    .unwrap();

    let mut watch = Command::new(env!("CARGO_BIN_EXE_specforge"));
    watch.args([
        "watch",
        "--path",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    let mut child = ChildGuard::spawn(
        guarded_command(&watch)
            .stdout(Stdio::piped())
            // stderr must be drained (or discarded): an un-read pipe fills and
            // blocks the child mid-run. Debug goes to a file for this test.
            .stderr(Stdio::from(fs::File::create("/tmp/watch-dbg.log").unwrap())),
    )
    .expect("watch spawns");

    // Wait for readiness (the `ready` event) with generous CI headroom.
    let stdout = child.take_stdout().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut ready = false;
    while !ready {
        if Instant::now() > deadline {
            panic!("watch never became ready");
        }
        let mut line = String::new();
        let n = std::io::BufRead::read_line(&mut reader, &mut line).unwrap_or(0);
        if n == 0 {
            panic!("watch exited before readiness");
        }
        eprintln!("[test] line: {}", line.trim());
        if line.contains("\"event\":\"ready\"") {
            ready = true;
        }
    }

    // Mutate the extension environment.
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("specforge.json")).unwrap())
            .unwrap();
    config["extensions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("@specforge/governance"));
    fs::write(dir.path().join("specforge.json"), config.to_string()).unwrap();

    // The next event batch must include `extensions_reloaded`.
    let deadline = Instant::now() + Duration::from_secs(20);
    let saw_reload;
    loop {
        if Instant::now() > deadline {
            panic!("no extensions_reloaded event after config change");
        }
        let mut line = String::new();
        let n = std::io::BufRead::read_line(&mut reader, &mut line).unwrap_or(0);
        if n == 0 {
            panic!("watch exited while waiting for reload event");
        }
        eprintln!("[test] waiting-for-reload line: {}", line.trim());
        let parsed = serde_json::from_str::<serde_json::Value>(line.trim());
        match parsed {
            Ok(v) if v["event"] == "extensions_reloaded" => {
                let exts = v["extensions"].as_array().unwrap();
                assert!(
                    exts.iter().any(|e| e == "@specforge/governance"),
                    "reloaded set should include the newly added extension: {}",
                    v
                );
                saw_reload = true;
                break;
            }
            _ => continue,
        }
    }
    assert!(saw_reload);

    // The watch loop never exits on its own; the guard stops and reaps it.
}
