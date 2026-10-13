//! H3/H5 gate (hardening-plan): `specforge watch --json` must react to a
//! `specforge.json` edit (extension list change) with an
//! `extensions_reloaded` event — no restart required (R-5).

use std::time::{Duration, Instant};

use crate::support::{Cli, TestProject};

#[test]
fn watch_reloads_extension_environment_on_config_change() {
    let dir = TestProject::new()
        .config(|c| c["name"] = "watch-reload".into())
        .enabling(&["@specforge/product"])
        .file(
            "main.spec",
            r#"feature watched "Watched feature" {
    status proposed
    priority high
}
"#,
        );

    let mut child = Cli::on(&dir).spawn(&["watch", "--format", "json"]);

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
    let mut config = dir.config_on_disk();
    config["extensions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("@specforge/governance"));
    dir.write("specforge.json", config.to_string());

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
