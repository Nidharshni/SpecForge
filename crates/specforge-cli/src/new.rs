//! `specforge new --extension <name>` — scaffold an SDK-authored extension
//! project (spec #21 follow-through / SDK adoption).
//!
//! The generated project mirrors `fixtures/greet-extension`: a cdylib crate
//! depending on `specforge-extension-sdk`, built as a `wasm32-wasip2`
//! component, with a `src/lib.rs` skeleton that builds and describes out of
//! the box. `specforge extension init` writes the same project.

use crate::terminal::{Terminal, outln};

use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use serde_json::json;
use specforge_common::codes;
use specforge_protocol_types::PackageName;
use std::path::{Path, PathBuf};

// `specforge new`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Extension name (e.g. @you/my-ext)
    name: String,

    /// Scaffold an extension project (SDK-authored wasm)
    #[arg(long, default_value_t = false)]
    extension: bool,

    /// Directory to scaffold into
    #[arg(long, default_value = ".")]
    path: PathBuf,

    #[command(flatten)]
    output: OutputArg,
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        new(
            &self.name,
            self.extension,
            &self.path,
            self.output.format,
            terminal,
        )
    }
}

fn new(
    name: &str,
    extension: bool,
    path: &Path,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    if !extension {
        return Refusal::of(format).coded(
            terminal,
            codes::E065,
            "only `--extension` scaffolding is supported right now",
        );
    }

    if let Err(why) = PackageName::parse(name) {
        return Refusal::of(format).coded(terminal, codes::E065, why.to_string());
    }

    let dir = target_dir(path, name);
    if dir.exists() {
        return Refusal::of(format).coded(
            terminal,
            codes::E065,
            format!("destination '{}' already exists", dir.display()),
        );
    }

    if let Err(message) = scaffold(&dir, name) {
        return Refusal::of(format).coded(terminal, codes::E066, message);
    }

    match format {
        OutputFormat::Json => {
            let output = json!({
                "action": "new",
                "kind": "extension",
                "name": name,
                "path": dir.display().to_string(),
            });
            terminal.json(&output);
        }
        OutputFormat::Human => {
            outln!(
                terminal,
                "scaffolded extension '{}' in {}",
                name,
                dir.display()
            );
            outln!(terminal);
            outln!(terminal, "next steps:");
            outln!(terminal, "  cd {}", dir.display());
            outln!(terminal, "  cargo build --release --target wasm32-wasip2");
            let wasm = format!(
                "./target/wasm32-wasip2/release/{}.wasm",
                crate_name(name).replace('-', "_")
            );
            outln!(
                terminal,
                "  specforge add {wasm}   # local-path install once built"
            );
        }
    }
    Exit::Passed
}

/// The crate- and directory-safe name: the last path segment, sanitized.
fn crate_name(name: &str) -> String {
    let last = name.rsplit('/').next().unwrap_or(name);
    let sanitized: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("ext-{}", sanitized)
    } else {
        sanitized
    }
}

/// The extension's short name, which routes its commands
/// (`specforge <short> <command>`): the crate name in lowercase kebab case,
/// starting with a letter.
pub(crate) fn short_name(name: &str) -> String {
    let kebab: String = crate_name(name)
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let kebab = kebab.trim_matches('-').to_string();
    match kebab.chars().next() {
        Some(c) if c.is_ascii_lowercase() => kebab,
        _ => format!("ext-{kebab}").trim_end_matches('-').to_string(),
    }
}

fn target_dir(path: &Path, name: &str) -> PathBuf {
    path.join(crate_name(name))
}

/// The files [`scaffold`] writes, relative to the project directory.
pub(crate) const SCAFFOLDED: &[&str] = &["Cargo.toml", ".cargo/config.toml", "src/lib.rs"];

/// Write an SDK extension crate declaring `name` into `dir`: its
/// [`SCAFFOLDED`] files.
pub(crate) fn scaffold(dir: &Path, name: &str) -> Result<(), String> {
    let short = short_name(name);
    let crate_name = crate_name(name);
    let src = dir.join("src");
    std::fs::create_dir_all(&src)
        .map_err(|e| format!("failed to create {}: {}", src.display(), e))?;
    std::fs::create_dir_all(dir.join(".cargo"))
        .map_err(|e| format!("failed to create .cargo: {}", e))?;

    let cargo_toml = format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
specforge-extension-sdk = "0.1"
wit-bindgen = "0.30"
serde_json = "1.0"
"#
    );
    std::fs::write(dir.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    std::fs::write(
        dir.join(".cargo").join("config.toml"),
        "[build]\ntarget = \"wasm32-wasip2\"\n",
    )
    .map_err(|e| format!("failed to write .cargo/config.toml: {}", e))?;

    let lib_rs = format!(
        r#"//! {name} — a SpecForge extension authored with the extension SDK.
//!
//! Build:  cargo build --release --target wasm32-wasip2
//! Install: specforge add ./path/to/this/dir

use specforge_extension_sdk::prelude::*;

#[specforge_extension_sdk::extension(
    name = "{name}",
    version = "0.1.0",
    short = "{short}",
    description = "TODO: one line saying what the extension is for"
)]
struct Extension;

impl Contributions for Extension {{
    fn contribute(c: &mut ContributionsBuilder) {{
        // Contribute an entity kind (delete if not needed):
        c.kind("thing", |k| {{
            k.description("TODO: what this entity represents");
            k.field("description", |f| {{
                f.field_type(FieldType::String)
                    .description("Free-form description");
            }});
        }});

        // Contribute a validation rule (delete if not needed):
        c.rule("W900", |r| {{
            r.check(CheckKind::MissingRequiredField);
            r.target_kind("thing");
            r.field("description");
            r.severity(ValidationSeverity::Warning);
            r.message_template("thing '{{id}}' is missing a description");
        }});

        // Contribute a command, declared with the function that answers it
        // (delete if not needed). It runs as `specforge {short} things` and is
        // the MCP tool `specforge.{short}.things`; its args are read through
        // the declaration.
        c.command("things", |cmd| {{
            cmd.title("List things")
                .description("Every thing, by id")
                .arg("limit", |a| {{
                    a.count().description("Return at most this many");
                }})
                .handler(|call| {{
                    let ids: Vec<&str> = call
                        .graph()
                        .nodes_of_kind("thing")
                        .take(call.count("limit").unwrap_or(100))
                        .map(|n| n.id.as_str())
                        .collect();
                    call.render(&serde_json::json!({{ "things": ids }}), |out| {{
                        for id in &ids {{
                            out.push_str(id);
                            out.push('\n');
                        }}
                    }})
                }});
        }});
    }}
}}

// Serves the protocol and the declared commands. Exports you answer by
// hand go to `handler = dispatch`, a function returning `None` for names it
// does not know.
specforge_extension_sdk::component_guest!(build = specforge_extension_build);
"#
    );
    std::fs::write(src.join("lib.rs"), lib_rs)
        .map_err(|e| format!("failed to write src/lib.rs: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_takes_last_segment_and_sanitizes() {
        assert_eq!(crate_name("@you/my-ext"), "my-ext");
        assert_eq!(crate_name("plain"), "plain");
        assert_eq!(crate_name("@weird scope/name!"), "name-");
    }

    #[test]
    fn the_short_name_is_lowercase_kebab_case() {
        assert_eq!(short_name("@you/my-ext"), "my-ext");
        assert_eq!(short_name("My_Ext"), "my-ext");
        assert_eq!(short_name("@x/4th-kind"), "ext-4th-kind");
    }

    #[test]
    fn crate_name_prefixes_leading_digit() {
        assert_eq!(crate_name("@x/4th-kind"), "ext-4th-kind");
    }

    #[specforge_test_macros::test(
        behavior = "scaffold_wasm_extension_project",
        verify = "the scaffold's extension name is a package name"
    )]
    fn the_scaffolds_extension_name_is_a_package_name() {
        for ok in ["@ok/name", "unscoped"] {
            assert!(PackageName::parse(ok).is_ok(), "{ok}");
        }
        for refused in ["@justscope", "   ", "Bad", "@a/../b"] {
            assert!(PackageName::parse(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn scaffold_writes_buildable_shape() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("my-ext");
        scaffold(&project, "@you/my-ext").unwrap();

        let cargo = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
        assert!(cargo.contains(r#"name = "my-ext""#));
        assert!(cargo.contains("crate-type = [\"cdylib\"]"));
        assert!(cargo.contains("specforge-extension-sdk"));

        let config = std::fs::read_to_string(project.join(".cargo/config.toml")).unwrap();
        assert!(config.contains("wasm32-wasip2"));

        let lib = std::fs::read_to_string(project.join("src/lib.rs")).unwrap();
        assert!(lib.contains("name = \"@you/my-ext\""));
        assert!(lib.contains("#[specforge_extension_sdk::extension("));
        assert!(lib.contains("impl Contributions for Extension"));
        assert!(lib.contains("component_guest!"));
        assert!(lib.contains("c.command(\"things\""));
        assert!(cargo.contains("serde_json"));
    }

    /// Every field type and check kind the scaffold names is one the
    /// host's registry build reads, so a freshly scaffolded extension loads
    /// without W019/W112 (the scaffold once used a check name it rejected).
    #[test]
    fn scaffold_uses_only_vocabulary_the_host_reads() {
        use specforge_protocol_types::{CheckKind, FieldType};

        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("my-ext");
        scaffold(&project, "@you/my-ext").unwrap();
        let lib = std::fs::read_to_string(project.join("src/lib.rs")).unwrap();

        let named = |prefix: &str| -> Vec<String> {
            lib.match_indices(prefix)
                .map(|(at, _)| {
                    lib[at + prefix.len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric())
                        .collect()
                })
                .collect()
        };
        let checks = named("CheckKind::");
        let types = named("FieldType::");
        assert!(!checks.is_empty() && !types.is_empty(), "{lib}");
        for name in checks {
            assert!(
                CheckKind::ALL.iter().any(|c| format!("{c:?}") == name),
                "scaffold names unknown CheckKind::{name}"
            );
        }
        for name in types {
            assert!(
                FieldType::ALL.iter().any(|t| format!("{t:?}") == name),
                "scaffold names unknown FieldType::{name}"
            );
        }
    }
}
