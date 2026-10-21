#![allow(
    clippy::unwrap_used,
    reason = "a test's unwrap is its assertion: a None or an Err fails the test"
)]

mod contracts;
mod corpus_syntax;
mod expr_test;
mod format_header;
mod format_version;
mod guide_spec_blocks;
mod lex;
mod methods_test;
mod name_spans;
mod parse_test;
mod ref_components;
mod robustness;
mod snapshot_tests;
mod statement_spans;
mod structural_keywords;
mod union_title;
