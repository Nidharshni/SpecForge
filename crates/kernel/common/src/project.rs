use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{Diagnostic, Exclude, ExcludeEntry, ExcludeError, codes};

/// What one `specforge.json` `extensions` entry enables: the one reading
/// of an entry that the runtime loading extensions, the environment
/// reading their declarations, the freshness inputs and the extension
/// operations share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionEntry<'a> {
    /// A builtin or an installed extension, by its name: `name`, or
    /// `name@version` as older `specforge add`s wrote it (`@acme/foo@1.2.0`
    /// names `@acme/foo`).
    Named(&'a str),
    /// A component loaded from a `.wasm` file: `path.wasm`, or
    /// `name=path.wasm` where `name` must be the name the component
    /// declares. A relative path is relative to the project root. The
    /// extension is the one the component declares (its handshake `name`).
    File {
        /// The name written before `=`, if any.
        name: Option<&'a str>,
        /// The path as written.
        path: &'a str,
    },
}

impl<'a> ExtensionEntry<'a> {
    /// Read an entry (surrounding whitespace ignored): one ending in
    /// `.wasm` names a file, any other an extension.
    pub fn parse(entry: &'a str) -> Self {
        let entry = entry.trim();
        if entry.ends_with(".wasm") {
            return match entry.split_once('=') {
                Some((name, path)) => ExtensionEntry::File {
                    name: Some(name.trim()),
                    path: path.trim(),
                },
                None => ExtensionEntry::File {
                    name: None,
                    path: entry,
                },
            };
        }
        // The one rule that splits `name@requirement`: the requirement of
        // a legacy entry is not read here.
        ExtensionEntry::Named(specforge_package::split_requirement(entry).0)
    }

    /// The package a named entry names, or why its name is none (a name
    /// that is no package name could never be installed, ADR 0036); `None`
    /// for a `.wasm` file entry, which names a file.
    pub fn package_name(
        &self,
    ) -> Option<Result<specforge_package::PackageName, specforge_package::PackageNameError>> {
        match *self {
            ExtensionEntry::Named(name) => Some(specforge_package::PackageName::parse(name)),
            ExtensionEntry::File { .. } => None,
        }
    }

    /// The extension's name as far as the entry alone says: a named
    /// entry's name, a file entry's name before `=`; `None` for a bare
    /// file (only its component says).
    pub fn name(&self) -> Option<&'a str> {
        match *self {
            ExtensionEntry::Named(name) => Some(name),
            ExtensionEntry::File { name, .. } => name,
        }
    }

    /// The file a file entry loads, a relative path resolved against
    /// `root` (the project root).
    pub fn file(&self, root: &Path) -> Option<PathBuf> {
        match *self {
            ExtensionEntry::Named(_) => None,
            ExtensionEntry::File { path, .. } => Some(root.join(path)),
        }
    }
}

/// The extension a `specforge.json` `extensions` entry names, as far as
/// the entry alone says ([`ExtensionEntry`]): `name`, or `name@version`
/// (`@acme/foo@1.2.0` names `@acme/foo`); for a `.wasm` file entry the
/// name before `=`, else the path as written.
pub fn extension_entry_name(entry: &str) -> &str {
    match ExtensionEntry::parse(entry) {
        ExtensionEntry::Named(name)
        | ExtensionEntry::File {
            name: Some(name), ..
        } => name,
        ExtensionEntry::File { name: None, path } => path,
    }
}

/// Parsed project configuration from specforge.json.
#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub name: Option<String>,
    pub version: Option<String>,
    pub spec_root: Option<String>,
    pub extensions: Vec<String>,
    /// What `exclude` leaves out of the project's sources (ADR 0055 D7):
    /// its usable entries, in file order; each unusable one is an E069.
    pub exclude: Exclude,
    pub inference: InferenceConfig,
    pub raw: Option<serde_json::Value>,
}

impl ProjectConfig {
    /// Where the sources of the project at `root` are: `spec_root` relative
    /// to `root` (the root itself when unset), with this config's `exclude`.
    pub fn sources(&self, root: &Path) -> crate::SourceRoot {
        let dir = match &self.spec_root {
            Some(spec_root) => root.join(spec_root),
            None => root.to_path_buf(),
        };
        crate::SourceRoot::new(dir, self.exclude.clone())
    }
}

/// Project-level inference hints that override/append to extension defaults.
#[derive(Debug, Clone, Default)]
pub struct InferenceConfig {
    pub global: Option<String>,
    pub kinds: HashMap<String, String>,
    pub density_threshold: Option<f64>,
}

/// One way the project's `specforge.json` is not used as written. Its
/// `Display` is the reason E069 names; for the problems that block an edit
/// ([`Self::blocks_edits`]) it is also the message `specforge add` and
/// `specforge remove` refuse with (`config_invalid`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigProblem {
    /// There, but not readable (permissions, not UTF-8, a directory).
    Unreadable { path: PathBuf, error: String },
    /// Not JSON (serde's line and column in `error`).
    NotJson { path: PathBuf, error: String },
    /// JSON, but not an object.
    NotAnObject { path: PathBuf },
    /// A key the config defines has the wrong JSON type (`name`,
    /// `version`, `spec_root`: a string; `extensions`, `exclude`: an
    /// array). That key's default is used; every other key is kept.
    WrongType {
        path: PathBuf,
        key: &'static str,
        expected: &'static str,
    },
    /// An item of `extensions` or `exclude` that is not a string. The item
    /// is ignored; the other items are kept.
    ItemNotAString {
        path: PathBuf,
        key: &'static str,
        index: usize,
        json: String,
    },
    /// An `exclude` item that can match no source. The item is ignored; the
    /// other items are kept.
    ExcludeEntry {
        path: PathBuf,
        index: usize,
        entry: String,
        error: ExcludeError,
    },
}

impl ConfigProblem {
    /// Nothing in the file could be used, or its `extensions` list could
    /// not: the compile loads no extension because of it.
    pub fn loads_nothing(&self) -> bool {
        match self {
            ConfigProblem::Unreadable { .. }
            | ConfigProblem::NotJson { .. }
            | ConfigProblem::NotAnObject { .. } => true,
            ConfigProblem::WrongType { key, .. } => *key == "extensions",
            ConfigProblem::ItemNotAString { .. } | ConfigProblem::ExcludeEntry { .. } => false,
        }
    }

    /// The file cannot be edited as an `extensions` list: `specforge add`
    /// and `specforge remove` refuse (`config_invalid`). The same set as
    /// [`Self::loads_nothing`]: an edit goes around a mistyped key or a
    /// non-string item.
    pub fn blocks_edits(&self) -> bool {
        self.loads_nothing()
    }

    /// The file does not say which files are the project's sources, or which
    /// extensions it loads: it could not be used at all, or `spec_root`,
    /// `exclude` or `extensions` has the wrong type. A run that rewrites the
    /// project's sources (`format`, `migrate`, `rename`) refuses
    /// (`config_invalid`): it would rewrite files the config leaves out, or
    /// migrate without the hooks of extensions it means to load. Every other
    /// problem leaves an item or a name the run does not read, and the run
    /// goes around it, reporting its E069.
    pub fn blocks_rewrites(&self) -> bool {
        self.loads_nothing()
            || matches!(
                self,
                ConfigProblem::WrongType { key, .. } if *key == "spec_root" || *key == "exclude"
            )
    }

    /// E069 for this problem: an error, "specforge.json can't be used: ...;
    /// no extension is loaded" for one that
    /// [`loads_nothing`](Self::loads_nothing), else "specforge.json: ...; it
    /// is ignored", with the suggestion to fix the file. The one text the
    /// compile, format and migrate report.
    pub fn diagnostic(&self) -> Diagnostic {
        let message = if self.loads_nothing() {
            format!("specforge.json can't be used: {self}; no extension is loaded")
        } else {
            format!("specforge.json: {self}; it is ignored")
        };
        Diagnostic::new(codes::E069, message).with_suggestion(format!(
            "fix specforge.json; `specforge explain {}` says what it must be",
            codes::E069
        ))
    }
}

impl std::fmt::Display for ConfigProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigProblem::Unreadable { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
            ConfigProblem::NotJson { path, error } => {
                write!(f, "{} is not valid JSON: {error}", path.display())
            }
            ConfigProblem::NotAnObject { path } => {
                write!(f, "{} must be a JSON object", path.display())
            }
            ConfigProblem::WrongType {
                path,
                key,
                expected,
            } => write!(f, "\"{key}\" in {} must be {expected}", path.display()),
            ConfigProblem::ItemNotAString {
                path,
                key,
                index,
                json,
            } => write!(
                f,
                "\"{key}\"[{index}] in {} must be a string, found {json}",
                path.display()
            ),
            ConfigProblem::ExcludeEntry {
                path,
                index,
                entry,
                error,
            } => write!(
                f,
                "\"exclude\"[{index}] in {}, \"{entry}\", {error}",
                path.display()
            ),
        }
    }
}

/// What reading `specforge.json` at a root gave: the config (the default
/// where a problem made a key or the whole file unusable), every problem in
/// file order, and whether the file exists at all.
#[derive(Debug, Clone, Default)]
pub struct ConfigRead {
    pub config: ProjectConfig,
    pub problems: Vec<ConfigProblem>,
    /// `specforge.json` exists at the root (readable or not). No file is no
    /// problem: a project with the default config.
    pub found: bool,
}

impl ConfigRead {
    /// One E069 per problem, in file order ([`ConfigProblem::diagnostic`]).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.problems
            .iter()
            .map(ConfigProblem::diagnostic)
            .collect()
    }
}

/// Read `specforge.json` in `project_root`: the config it gives, and every
/// way it is not used as written ([`ConfigProblem`]), in file order. A
/// missing file is the default config with no problem.
pub fn read_project_config(project_root: &Path) -> ConfigRead {
    let path = project_root.join("specforge.json");
    let unusable = |problem: ConfigProblem| ConfigRead {
        config: ProjectConfig::default(),
        problems: vec![problem],
        found: true,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ConfigRead::default(),
        Err(e) => {
            return unusable(ConfigProblem::Unreadable {
                path,
                error: e.to_string(),
            });
        }
    };
    parse_project_config(&path, &content)
}

/// Parse `content`, the text of the `specforge.json` at `path` (named in
/// each problem): the config it gives and every way it is not used as
/// written, in file order. `found` is true. [`read_project_config`] reads
/// the file, then parses it.
pub fn parse_project_config(path: &Path, content: &str) -> ConfigRead {
    let path = path.to_path_buf();
    let unusable = |problem: ConfigProblem| ConfigRead {
        config: ProjectConfig::default(),
        problems: vec![problem],
        found: true,
    };
    let value: serde_json::Value = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(e) => {
            return unusable(ConfigProblem::NotJson {
                path,
                error: e.to_string(),
            });
        }
    };
    let Some(object) = value.as_object() else {
        return unusable(ConfigProblem::NotAnObject { path });
    };

    let mut problems = Vec::new();
    let mut config = ProjectConfig::default();
    // The keys in the order the file writes them, so the problems are
    // reported in file order.
    for key in keys_in_file_order(content) {
        let Some(item) = object.get(&key) else {
            continue;
        };
        match key.as_str() {
            "name" => config.name = string_key(&path, "name", item, &mut problems),
            "version" => config.version = string_key(&path, "version", item, &mut problems),
            "spec_root" => config.spec_root = string_key(&path, "spec_root", item, &mut problems),
            "extensions" => config.extensions = list_key(&path, "extensions", item, &mut problems),
            "exclude" => config.exclude = exclude_key(&path, item, &mut problems),
            _ => {}
        }
    }
    config.inference = parse_inference_config(&value);
    config.raw = Some(value);
    ConfigRead {
        config,
        problems,
        found: true,
    }
}

/// A key whose value must be a string: `None`, and a problem, otherwise.
fn string_key(
    path: &Path,
    key: &'static str,
    value: &serde_json::Value,
    problems: &mut Vec<ConfigProblem>,
) -> Option<String> {
    match value.as_str() {
        Some(text) => Some(text.to_string()),
        None => {
            problems.push(ConfigProblem::WrongType {
                path: path.to_path_buf(),
                key,
                expected: "a string",
            });
            None
        }
    }
}

/// A key whose value must be an array of strings: its strings with their
/// index in the array, and one problem per item that is not one; empty, and
/// a problem, when it is not an array.
fn string_items(
    path: &Path,
    key: &'static str,
    value: &serde_json::Value,
    problems: &mut Vec<ConfigProblem>,
) -> Vec<(usize, String)> {
    let Some(items) = value.as_array() else {
        problems.push(ConfigProblem::WrongType {
            path: path.to_path_buf(),
            key,
            expected: "an array",
        });
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item.as_str() {
            Some(text) => Some((index, text.to_string())),
            None => {
                problems.push(ConfigProblem::ItemNotAString {
                    path: path.to_path_buf(),
                    key,
                    index,
                    json: item.to_string(),
                });
                None
            }
        })
        .collect()
}

/// A key whose value must be an array of strings: its strings, as
/// [`string_items`] reads them.
fn list_key(
    path: &Path,
    key: &'static str,
    value: &serde_json::Value,
    problems: &mut Vec<ConfigProblem>,
) -> Vec<String> {
    string_items(path, key, value, problems)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

/// The `exclude` key: its usable entries, with one problem per item that is
/// not a string and one per string that can match no source.
fn exclude_key(
    path: &Path,
    value: &serde_json::Value,
    problems: &mut Vec<ConfigProblem>,
) -> Exclude {
    let mut entries = Vec::new();
    for (index, entry) in string_items(path, "exclude", value, problems) {
        match ExcludeEntry::parse(&entry) {
            Ok(parsed) => entries.push(parsed),
            Err(error) => problems.push(ConfigProblem::ExcludeEntry {
                path: path.to_path_buf(),
                index,
                entry,
                error,
            }),
        }
    }
    Exclude::new(entries)
}

/// The top-level keys of the JSON object `content`, in the order it writes
/// them (each once, at its first occurrence). Empty when it is not an
/// object.
fn keys_in_file_order(content: &str) -> Vec<String> {
    struct Keys(Vec<String>);

    impl<'de> serde::Deserialize<'de> for Keys {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Keys;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a JSON object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Keys, A::Error> {
                    let mut keys: Vec<String> = Vec::new();
                    while let Some(key) = map.next_key::<String>()? {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        if !keys.contains(&key) {
                            keys.push(key);
                        }
                    }
                    Ok(Keys(keys))
                }
            }
            deserializer.deserialize_map(Visitor)
        }
    }

    serde_json::from_str::<Keys>(content)
        .map(|keys| keys.0)
        .unwrap_or_default()
}

fn parse_inference_config(value: &serde_json::Value) -> InferenceConfig {
    let obj = match value.get("inference").and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return InferenceConfig::default(),
    };

    let global = obj
        .get("global")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut kinds = HashMap::new();
    for (key, val) in obj {
        if key == "global" {
            continue;
        }
        if let Some(s) = val.as_str() {
            kinds.insert(key.clone(), s.to_string());
        }
    }

    let density_threshold = obj.get("density_threshold").and_then(|v| v.as_f64());

    InferenceConfig {
        global,
        kinds,
        density_threshold,
    }
}

/// Why `name` can't name a project, or `Ok`: it must be non-empty, at most
/// 214 characters, free of whitespace, and not start with `.` or `-`.
pub fn validate_project_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("name must not be empty");
    }
    if name.len() > 214 {
        return Err("name must not exceed 214 characters");
    }
    if name.starts_with('.') || name.starts_with('-') {
        return Err("name must not start with '.' or '-'");
    }
    if name.contains(|c: char| c.is_whitespace()) {
        return Err("name must not contain whitespace");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
