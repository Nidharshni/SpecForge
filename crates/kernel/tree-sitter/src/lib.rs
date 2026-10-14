//! The tree-sitter grammar of the `.spec` language: `LANGUAGE` (the parser generated from `grammar.js`, with its
//! external scanner) and the names of its node kinds (`kind`) and fields (`field`), which the parser and the
//! formatter name nodes by. `queries/` holds the editor queries.

use tree_sitter_language::LanguageFn;

pub mod field;
pub mod kind;

#[expect(
    unsafe_code,
    reason = "the grammar is C: tree-sitter generates `tree_sitter_specforge`, which takes nothing and returns the static language"
)]
unsafe extern "C" {
    fn tree_sitter_specforge() -> *const ();
}

#[expect(
    unsafe_code,
    reason = "LanguageFn::from_raw needs a function returning a tree-sitter language, which the generated parser is"
)]
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_specforge) };
