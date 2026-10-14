use super::Previous;

/// A copy of the previous tree with the [`tree_sitter::InputEdit`] for a
/// whole-file replacement applied, so a reparse can reuse unchanged subtrees:
/// the longest common byte prefix and suffix of the two texts.
pub(super) fn edited(previous: Previous<'_>, new_src: &str) -> tree_sitter::Tree {
    let (old_tree, old_src) = (previous.tree, previous.text);
    if old_src == new_src {
        return old_tree.clone();
    }
    let mut prefix = 0;
    let max_prefix = old_src.len().min(new_src.len());
    while prefix < max_prefix && old_src.as_bytes()[prefix] == new_src.as_bytes()[prefix] {
        prefix += 1;
    }
    while prefix > 0 && !old_src.is_char_boundary(prefix) {
        prefix -= 1;
    }

    let mut suffix = 0;
    let max_suffix = max_prefix - prefix;
    while suffix < max_suffix
        && old_src.as_bytes()[old_src.len() - 1 - suffix]
            == new_src.as_bytes()[new_src.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0 && !new_src.is_char_boundary(new_src.len() - suffix) {
        suffix -= 1;
    }

    let start_byte = prefix;
    let old_end_byte = old_src.len() - suffix;
    let new_end_byte = new_src.len() - suffix;

    let point = |src: &str, byte: usize| {
        let before = &src[..byte];
        let row = before.matches('\n').count();
        let column = byte - before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        tree_sitter::Point { row, column }
    };

    let mut tree = old_tree.clone();
    tree.edit(&tree_sitter::InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: point(old_src, start_byte),
        old_end_position: point(old_src, old_end_byte),
        new_end_position: point(new_src, new_end_byte),
    });
    tree
}
