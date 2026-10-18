//! The kinds the grammar itself parses, not an extension: a project's root
//! container, an external reference, an import, a definition block.

/// The project's root container.
pub const SPEC: &str = "spec";
/// An external reference (`ref gh.issue:1 "..."`).
pub const REF: &str = "ref";
/// An import.
pub const USE: &str = "use";
/// A definition block (not supported: reported as W143).
pub const DEFINE: &str = "define";

/// File defaults (ADR 0149): a structural form, parsed by the generic entity
/// block, never a node.
pub const DEFAULTS: &str = "defaults";

/// Every structural keyword: never an extension's kind, always reserved as
/// an ID (E013).
pub const KEYWORDS: [&str; 5] = [SPEC, REF, USE, DEFINE, DEFAULTS];

/// The structural keywords that become graph nodes; `use` is an import and
/// `define` is never a node (W143).
pub const NODE_KINDS: [&str; 2] = [REF, SPEC];

/// `keyword` is one of [`KEYWORDS`].
pub fn is_structural(keyword: &str) -> bool {
    KEYWORDS.contains(&keyword)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_structural_keywords_are_the_grammar_forms_and_defaults() {
        assert_eq!(KEYWORDS, ["spec", "ref", "use", "define", "defaults"]);
        assert!(KEYWORDS.iter().all(|k| is_structural(k)));
        assert!(!is_structural("behavior"));
    }
}
