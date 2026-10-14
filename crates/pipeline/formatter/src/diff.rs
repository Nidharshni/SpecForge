/// A unified diff for a single file.
#[derive(Debug, Clone)]
pub struct FormatDiff {
    pub file_path: String,
    pub diff_text: String,
    pub insertions: usize,
    pub deletions: usize,
}

/// Generate a unified diff between original and formatted content: a real
/// line diff (Myers, via `similar`) with 3 lines of context, so a line
/// never shows up as both removed and added.
pub fn unified_diff(file_path: &str, original: &str, formatted: &str) -> FormatDiff {
    if original == formatted {
        return FormatDiff {
            file_path: file_path.to_string(),
            diff_text: String::new(),
            insertions: 0,
            deletions: 0,
        };
    }
    let diff = similar::TextDiff::from_lines(original, formatted);
    let (mut insertions, mut deletions) = (0, 0);
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => insertions += 1,
            similar::ChangeTag::Delete => deletions += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    let diff_text = diff
        .unified_diff()
        .context_radius(3)
        .header(file_path, file_path)
        .to_string();
    FormatDiff {
        file_path: file_path.to_string(),
        diff_text,
        insertions,
        deletions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "show_formatting_diff",
        verify = "diff output uses unified format"
    )]
    fn test_unified_diff_format() {
        let diff = unified_diff(
            "spec/test.spec",
            "  behavior foo \"Foo\" {\n  contract \"a\"\n}\n",
            "behavior foo \"Foo\" {\n  contract \"a\"\n}\n",
        );
        assert!(diff.diff_text.contains("--- spec/test.spec"));
        assert!(diff.diff_text.contains("+++ spec/test.spec"));
        assert!(diff.diff_text.contains("@@"));
    }

    #[specforge_test_macros::test(
        behavior = "show_formatting_diff",
        verify = "unchanged files produce no diff output"
    )]
    fn test_diff_unchanged_files_empty() {
        let diff = unified_diff("test.spec", "hello\n", "hello\n");
        assert!(diff.diff_text.is_empty());
        assert_eq!(diff.insertions, 0);
        assert_eq!(diff.deletions, 0);
    }

    #[specforge_test_macros::test(
        behavior = "show_formatting_diff",
        verify = "diff output uses unified format"
    )]
    fn test_diff_counts_insertions_deletions() {
        let diff = unified_diff("test.spec", "  line1\n  line2\n", "line1\nline2\n");
        assert!(diff.insertions > 0 || diff.deletions > 0);
    }

    #[specforge_test_macros::test(
        behavior = "show_formatting_diff",
        verify = "an unchanged line is shown as context, never as removed and added"
    )]
    fn a_moved_line_is_one_change_not_two() {
        let original = "use \"b\"\nuse \"a\"\n\nbehavior x \"X\" {\n  contract \"c\"\n\n  verify unit \"v\"\n}\n";
        let formatted = "use \"a\"\nuse \"b\"\n\nbehavior x \"X\" {\n  contract \"c\"\n  verify unit \"v\"\n}\n";
        let diff = unified_diff("x.spec", original, formatted);
        // Unchanged lines are context only; the moved import is one removal
        // and one insertion, the dropped blank line one removal.
        assert_eq!(
            diff.diff_text,
            concat!(
                "--- x.spec\n+++ x.spec\n@@ -1,8 +1,7 @@\n",
                "+use \"a\"\n use \"b\"\n-use \"a\"\n \n behavior x \"X\" {\n",
                "   contract \"c\"\n-\n   verify unit \"v\"\n }\n",
            )
        );
        assert_eq!((diff.insertions, diff.deletions), (1, 2));
    }
}
