//! C1-09: docs/guides/authoring-spec-files.md promises "every code block in
//! this guide compiles". This test enforces the promise: every ```spec
//! fenced block is extracted and parsed — a guide edit that invalidates a
//! snippet fails here instead of silently drifting.

use specforge_parser::parse;

/// The guide, from the repository root.
const GUIDE: &str = "docs/guides/authoring-spec-files.md";

/// Extract the contents of every fenced block tagged ```spec.
fn spec_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut start_line = 0usize;
    let mut buffer = String::new();
    for (idx, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        if !in_block && trimmed.starts_with("```spec") {
            in_block = true;
            start_line = idx + 1;
            buffer.clear();
        } else if in_block && trimmed.starts_with("```") {
            in_block = false;
            blocks.push((start_line, buffer.clone()));
        } else if in_block {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }
    assert!(!in_block, "unclosed ```spec fence in {GUIDE}");
    blocks
}

#[test]
fn every_guide_spec_block_parses() {
    let markdown = std::fs::read_to_string(specforge_repo_root::repo_root().join(GUIDE))
        .unwrap_or_else(|e| panic!("guide must exist at {GUIDE}: {e}"));
    let blocks = spec_blocks(&markdown);
    assert!(
        blocks.len() >= 15,
        "guide should still contain its worked snippets (found {})",
        blocks.len()
    );

    let mut failures = Vec::new();
    for (line, block) in &blocks {
        let file = format!("{GUIDE}:{line}");
        let parsed = parse(block, &file);
        let errors: Vec<String> = parsed
            .errors
            .iter()
            .map(|e| format!("{file}: {e:?}"))
            .collect();
        failures.extend(errors);
    }

    assert!(
        failures.is_empty(),
        "guide spec snippets no longer parse (C1-09 promise broken):\n{}",
        failures.join("\n")
    );
}
