//! The tree-sitter grammar of the `.spec` language: `LANGUAGE` (the parser generated from `grammar.js`, with its
//! external scanner) and the names of its node kinds (`kind`) and fields (`field`), which the parser and the
//! formatter name nodes by. `queries/` holds the editor queries.

use tree_sitter_language::LanguageFn;

pub mod field;
pub mod kind;

unsafe extern "C" {
    fn tree_sitter_specforge() -> *const ();
}

pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_specforge) };
