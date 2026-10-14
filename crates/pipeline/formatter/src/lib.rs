pub mod config;
pub mod diff;
pub mod engine;

pub use config::{FormatConfig, load_config};
pub use diff::{FormatDiff, unified_diff};
pub use engine::{FormatResult, format_range, format_source};
