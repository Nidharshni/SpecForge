//! `docs/extension-sdk.md` names only what the SDK has.
#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

fn doc() -> String {
    std::fs::read_to_string(specforge_repo_root::repo_root().join("docs/extension-sdk.md"))
        .expect("the SDK doc exists")
}

/// The text of every fenced ```rust block of `doc`, in order, with the line each starts after.
fn rust_blocks(doc: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (number, line) in doc.lines().enumerate() {
        match (&mut current, line.trim_end()) {
            (None, "```rust") => current = Some((number, String::new())),
            (Some(_), "```") => blocks.extend(current.take()),
            (Some((_, text)), line) => {
                text.push_str(line);
                text.push('\n');
            }
            _ => {}
        }
    }
    blocks
}

#[test]
fn the_docs_complete_example_is_the_greet_fixture() {
    let doc = doc();
    let heading = doc
        .lines()
        .position(|l| l == "### Complete Example")
        .expect("the doc has a Complete Example");
    let (_, example) = rust_blocks(&doc)
        .into_iter()
        .find(|(start, _)| *start > heading)
        .expect("a rust block follows the heading");
    let fixture = std::fs::read_to_string(
        specforge_repo_root::repo_root().join("fixtures/greet-extension/src/contributions.rs"),
    )
    .expect("the greet fixture exists");
    assert_eq!(
        example.trim_end(),
        fixture.trim_end(),
        "the doc's example is fixtures/greet-extension/src/contributions.rs"
    );
}

#[test]
fn the_docs_name_no_attribute_the_sdk_lacks() {
    const ALLOWED: &[&str] = &[
        "extension",
        "specforge_extension_sdk::extension",
        "cfg",
        "test",
        "specforge_test",
        "derive",
        "allow",
    ];
    for (_, block) in rust_blocks(&doc()) {
        for line in block.lines() {
            let Some(rest) = line.trim_start().strip_prefix("#[") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            assert!(
                ALLOWED.contains(&name.as_str()),
                "the doc uses #[{name}], which the SDK does not have: {line}"
            );
        }
    }
}
