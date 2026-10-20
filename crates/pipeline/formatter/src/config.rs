use specforge_common::{Diagnostic, codes};
use std::path::Path;

/// Formatter configuration, loaded from `.specforgefmt.toml` or defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatConfig {
    pub indent_width: usize,
    pub use_tabs: bool,
    pub max_width: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            use_tabs: false,
            max_width: 100,
        }
    }
}

impl FormatConfig {
    /// Returns the indent string for one level of indentation.
    pub fn indent_str(&self) -> String {
        if self.use_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.indent_width)
        }
    }
}

/// The name of a formatter configuration file: looked up from a document's directory up to its project root by
/// `specforge_ops`' configuration lookup, which hands [`parse`] what its disk read (ADR 0169).
pub const FILE: &str = ".specforgefmt.toml";

/// The configuration `read` gives: the bytes of the `.specforgefmt.toml` at `path`, or why they could not be read.
/// A read that failed, text that is not UTF-8 or not TOML, and every invalid value are a W141 naming `path`; what
/// is invalid falls back to its default. Never opens a file: `path` only names it in the messages.
pub fn parse(path: &Path, read: std::io::Result<Vec<u8>>) -> (FormatConfig, Vec<Diagnostic>) {
    let invalid = |message: String| Diagnostic::new(codes::W141, message);
    let content = match read.and_then(|bytes| {
        String::from_utf8(bytes).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
    }) {
        Ok(content) => content,
        Err(e) => {
            let message = format!("Failed to read config file {}: {e}", path.display());
            return (FormatConfig::default(), vec![invalid(message)]);
        }
    };
    let table: toml::Table = match content.parse() {
        Ok(table) => table,
        Err(e) => {
            let message = format!("Invalid TOML in {}: {e}", path.display());
            return (FormatConfig::default(), vec![invalid(message)]);
        }
    };

    let mut config = FormatConfig::default();
    let mut diagnostics = Vec::new();

    if let Some(val) = table.get("indent_width") {
        match val.as_integer() {
            Some(n) if (1..=16).contains(&n) => config.indent_width = n as usize,
            _ => diagnostics.push(
                invalid(format!(
                    "Invalid indent_width in {}: expected integer 1-16, using default {}",
                    path.display(),
                    config.indent_width
                ))
                .with_suggestion("indent_width must be an integer between 1 and 16"),
            ),
        }
    }

    if let Some(val) = table.get("use_tabs") {
        match val.as_bool() {
            Some(b) => config.use_tabs = b,
            None => diagnostics.push(
                invalid(format!(
                    "Invalid use_tabs in {}: expected boolean, using default {}",
                    path.display(),
                    config.use_tabs
                ))
                .with_suggestion("use_tabs must be true or false"),
            ),
        }
    }

    if let Some(val) = table.get("max_width") {
        match val.as_integer() {
            Some(n) if (40..=200).contains(&n) => config.max_width = n as usize,
            _ => diagnostics.push(
                invalid(format!(
                    "Invalid max_width in {}: expected integer 40-200, using default {}",
                    path.display(),
                    config.max_width
                ))
                .with_suggestion("max_width must be an integer between 40 and 200"),
            ),
        }
    }

    (config, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(source: &str) -> std::io::Result<Vec<u8>> {
        Ok(source.as_bytes().to_vec())
    }

    #[specforge_test_macros::test(
        invariant = "config_defaults_valid",
        verify = "default FormatConfig passes validation"
    )]
    fn the_default_config_reads_back_without_a_diagnostic() {
        let d = FormatConfig::default();
        let source = format!(
            "indent_width = {}\nuse_tabs = {}\nmax_width = {}\n",
            d.indent_width, d.use_tabs, d.max_width
        );

        let (config, diagnostics) = parse(Path::new(FILE), text(&source));

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(config, d);
    }

    #[specforge_test_macros::test(
        behavior = "load_format_config",
        verify = "missing config file uses defaults"
    )]
    fn test_default_config() {
        let config = FormatConfig::default();
        assert_eq!(config.indent_width, 2);
        assert!(!config.use_tabs);
        assert_eq!(config.max_width, 100);
    }

    #[specforge_test_macros::test(
        behavior = "load_format_config",
        verify = "invalid indent_width produces diagnostic and uses default"
    )]
    #[specforge_test_macros::test(
        invariant = "config_defaults_valid",
        verify = "fallback from invalid config produces usable FormatConfig"
    )]
    fn parse_reports_an_invalid_value_and_keeps_the_valid_ones() {
        let path = Path::new("/p/.specforgefmt.toml");

        let (config, diags) = parse(path, text("indent_width = 99\nuse_tabs = true\n"));

        assert_eq!(config.indent_width, 2, "the default replaces 99");
        assert!(config.use_tabs, "a valid value still applies");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, "W141");
        assert!(diags[0].message.contains("indent_width"), "{diags:?}");
        assert!(
            diags[0].message.contains(&path.display().to_string()),
            "the message names the file: {diags:?}"
        );
    }

    #[specforge_test_macros::test(
        behavior = "load_format_config",
        verify = "a configuration the disk cannot read is a W141 naming why, and its defaults apply"
    )]
    fn parse_reports_a_failed_read_as_one_w141_naming_the_file_and_the_reason() {
        let path = Path::new("/p/.specforgefmt.toml");
        let refused = std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "it has 2 names, so no bound can say where its content is",
        );

        let (config, diags) = parse(path, Err(refused));

        assert_eq!(config, FormatConfig::default());
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, "W141");
        assert!(
            diags[0].message.contains("/p/.specforgefmt.toml"),
            "{diags:?}"
        );
        assert!(diags[0].message.contains("2 names"), "{diags:?}");
    }

    #[specforge_test_macros::test(
        behavior = "load_format_config",
        verify = "a configuration the disk cannot read is a W141 naming why, and its defaults apply"
    )]
    fn parse_reports_text_that_is_not_utf8() {
        let (config, diags) = parse(Path::new(FILE), Ok(vec![0xff]));

        assert_eq!(config, FormatConfig::default());
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("valid UTF-8"), "{diags:?}");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn test_indent_str_spaces() {
        let config = FormatConfig {
            indent_width: 4,
            use_tabs: false,
            max_width: 100,
        };
        assert_eq!(config.indent_str(), "    ");
    }

    #[specforge_test_macros::test(
        behavior = "apply_format_rules",
        verify = "indentation rules normalize to configured indent style"
    )]
    fn test_indent_str_tabs() {
        let config = FormatConfig {
            indent_width: 4,
            use_tabs: true,
            max_width: 100,
        };
        assert_eq!(config.indent_str(), "\t");
    }
}
