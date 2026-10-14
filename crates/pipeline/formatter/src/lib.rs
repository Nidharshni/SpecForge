//! The `.spec` formatter: `format_source` lays out a whole document and `format_range` the blocks a range touches,
//! in the canonical layout its `FormatConfig` (`.specforgefmt.toml`) sets; `unified_diff` shows what it would
//! change. It reads the parser's recovered syntax, so a form the grammar could not read whole is kept as written
//! (W142, ADR 0063). Operations reach it through `specforge_ops::format` (ADR 0021).

pub mod config;
pub mod diff;
pub mod engine;

pub use config::{FormatConfig, load_config};
pub use diff::{FormatDiff, unified_diff};
pub use engine::{FormatResult, format_range, format_source};
