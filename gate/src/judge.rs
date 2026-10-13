//! Pure functions the checks call; each is tested on literal input.

/// Whether a crate's sources hold a runnable doctest (was `gate.sh:54-66`'s awk): a `///` or `//!` fence opening with
/// no language, `rust`, `compile_fail`, `should_panic` or `no_run`. A closing fence never reopens.
pub fn has_runnable_doctest(sources: &[&str]) -> bool {
    sources.iter().any(|source| {
        let mut open = false;
        source.lines().any(|line| {
            let Some(after) = fence_language(line) else {
                return false;
            };
            if open {
                open = false;
                return false;
            }
            open = true;
            matches!(
                after.trim_end_matches([' ', '\t']),
                "" | "rust" | "compile_fail" | "should_panic" | "no_run"
            )
        })
    })
}

/// The text after the backticks of a doc-comment fence line (`/// ```rust` gives `rust`).
fn fence_language(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches([' ', '\t']).strip_prefix("//")?;
    let rest = rest.strip_prefix('/').or_else(|| rest.strip_prefix('!'))?;
    rest.trim_start_matches([' ', '\t']).strip_prefix("```")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runnable_fence_marks_a_doctest_crate() {
        for lang in ["", "rust", "no_run", "compile_fail", "should_panic"] {
            let source = format!("/// Doc.\n/// ```{lang}\n/// let a = 1;\n/// ```\nfn f() {{}}\n");
            assert!(has_runnable_doctest(&[&source]), "```{lang}");
        }
        for lang in ["text", "ignore", "json", "rust,ignore"] {
            let source = format!("//! ```{lang}\n//! x\n//! ```\n");
            assert!(!has_runnable_doctest(&[&source]), "```{lang}");
        }
        // The closing fence of a text block does not open a runnable one.
        assert!(!has_runnable_doctest(&["/// ```text\n/// x\n/// ```\n"]));
        // A plain comment is not a doc comment.
        assert!(!has_runnable_doctest(&["// ```\n// x\n// ```\n"]));
        assert!(!has_runnable_doctest(&[]));
    }
}
