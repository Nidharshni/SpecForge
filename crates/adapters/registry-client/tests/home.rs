//! The credential store reads the directory it is given, never the developer's `HOME` (ADR 0178).

#[test]
#[ignore = "flipped by T5"]
fn the_store_tests_touch_nothing_under_home() {
    let home = tempfile::tempdir().unwrap();
    let secret = home.path().join(".specforge/secrets/temp.json");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&secret, r#"{"secret":"keep-me"}"#).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["credentials::", "--test-threads", "1"])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        std::fs::read_to_string(&secret).ok().as_deref(),
        Some(r#"{"secret":"keep-me"}"#),
        "a store test deleted a secret under HOME"
    );
}
