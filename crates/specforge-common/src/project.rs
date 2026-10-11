use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Whether `dir` is a project root: it holds `specforge.json` or `specforge.spec`.
pub fn is_project_root(dir: &Path) -> bool {
    dir.join("specforge.json").exists() || dir.join("specforge.spec").exists()
}

/// Locate the project root by walking from `start` upward to the filesystem root.
///
/// At each directory level, checks for `specforge.json` (preferred) then `specforge.spec`.
/// The first directory containing either file wins (closest-wins semantics).
/// Symlinks are resolved before comparison to avoid infinite loops.
///
/// Returns `None` if neither file is found in any ancestor.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().ok()?;

    loop {
        if is_project_root(&current) {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// The project `path` is in: its nearest enclosing project
/// ([`find_project_root`], `path` included), else `path` itself. The one
/// rule for what format, migrate and an MCP call's `path` act on, so a
/// directory that is no project is its own root and gets the default
/// configuration.
pub fn project_root_of(path: &Path) -> PathBuf {
    find_project_root(path).unwrap_or_else(|| path.to_path_buf())
}

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
    /// Path substrings excluded from `.spec` discovery (C4-04): matched
    /// against the workspace-relative path by the shared discovery walker.
    pub exclude: Vec<String>,
    pub inference: InferenceConfig,
    pub raw: Option<serde_json::Value>,
}

impl ProjectConfig {
    /// Where this project's `.spec` files are discovered: `spec_root`, relative
    /// to `root`, or `root` itself when unset.
    pub fn spec_root_in(&self, root: &Path) -> PathBuf {
        match &self.spec_root {
            Some(spec_root) => root.join(spec_root),
            None => root.to_path_buf(),
        }
    }

    /// The project's sources: every `.spec` file under the spec root that
    /// discovery keeps (no skipped directory, no `exclude` entry), sorted —
    /// the files a compile reads, and what format and migrate rewrite (see
    /// [`crate::walk_spec_files`]).
    pub fn spec_files(&self, root: &Path) -> Vec<PathBuf> {
        crate::discover_spec_files(&self.spec_root_in(root), &self.exclude)
    }

    /// Whether `path`, a `.spec` file under `spec_root`, is left out by an
    /// `exclude` entry or a skipped directory (false outside `spec_root`).
    pub fn excludes(&self, spec_root: &Path, path: &Path) -> bool {
        path.strip_prefix(spec_root)
            .is_ok_and(|relative| !crate::is_discovered(&relative.to_string_lossy(), &self.exclude))
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
            ConfigProblem::ItemNotAString { .. } => false,
        }
    }

    /// The file cannot be edited as an `extensions` list: `specforge add`
    /// and `specforge remove` refuse (`config_invalid`). The same set as
    /// [`Self::loads_nothing`]: an edit goes around a mistyped key or a
    /// non-string item.
    pub fn blocks_edits(&self) -> bool {
        self.loads_nothing()
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
            "exclude" => config.exclude = list_key(&path, "exclude", item, &mut problems),
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

/// Load project configuration from specforge.json in the given directory:
/// [`read_project_config`]'s config, the problems dropped (a best-effort
/// read: the default for whatever is unusable). Returns a default config if
/// the file doesn't exist.
pub fn load_project_config(project_root: &Path) -> ProjectConfig {
    read_project_config(project_root).config
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

/// A key whose value must be an array of strings: its strings, with one
/// problem per item that is not one; empty, and a problem, when it is not
/// an array.
fn list_key(
    path: &Path,
    key: &'static str,
    value: &serde_json::Value,
    problems: &mut Vec<ConfigProblem>,
) -> Vec<String> {
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
            Some(text) => Some(text.to_string()),
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
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "load_extension_manifests",
        verify = "E069 names why specforge.json can't be used"
    )]
    fn parsing_a_config_text_is_reading_it_from_its_file() {
        let texts = [
            r#"{"name": "p", "spec_root": "spec", "exclude": ["gen"], "extensions": ["@a/b"],
                "inference": {"global": "g", "density_threshold": 0.5}}"#,
            r#"{"name": 5}"#,
            r#"{ "extensions": ["#,
            "[1]",
        ];
        for text in texts {
            let dir = tempfile::TempDir::new().unwrap();
            let path = dir.path().join("specforge.json");
            std::fs::write(&path, text).unwrap();
            let read = read_project_config(dir.path());
            let parsed = parse_project_config(&path, text);
            assert!(parsed.found, "{text}");
            assert_eq!(format!("{read:?}"), format!("{parsed:?}"), "{text}");
        }
    }

    #[test]
    fn project_root_of_is_the_nearest_project_else_the_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let inner = root.join("project/spec/deep");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(root.join("project/specforge.json"), "{}").unwrap();
        let loose = root.join("loose");
        std::fs::create_dir_all(&loose).unwrap();

        // Inside a project: its root, from the root and from below it.
        assert_eq!(project_root_of(&root.join("project")), root.join("project"));
        assert_eq!(project_root_of(&inner), root.join("project"));
        // Outside any project: the directory itself.
        assert_eq!(project_root_of(&loose), loose);
        // A path that does not exist is itself.
        let missing = root.join("nowhere");
        assert_eq!(project_root_of(&missing), missing);
    }

    #[test]
    fn spec_root_in_defaults_to_the_root() {
        let root = Path::new("/p");
        let unset = ProjectConfig::default();
        let set = ProjectConfig {
            spec_root: Some("specs".into()),
            ..ProjectConfig::default()
        };

        assert_eq!(unset.spec_root_in(root), root);
        assert_eq!(set.spec_root_in(root), root.join("specs"));
    }

    #[test]
    fn spec_files_keep_only_the_sources() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        for file in [
            "specs/a.spec",
            "specs/sub/b.spec",
            "specs/drafts/d.spec",
            "specs/target/t.spec",
            "specs/notes.md",
            "fixtures/fx.spec",
        ] {
            std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            std::fs::write(root.join(file), "").unwrap();
        }
        let config = ProjectConfig {
            spec_root: Some("specs".into()),
            exclude: vec!["drafts".into()],
            ..ProjectConfig::default()
        };
        let spec_root = config.spec_root_in(root);

        assert_eq!(
            config.spec_files(root),
            [root.join("specs/a.spec"), root.join("specs/sub/b.spec")]
        );
        assert!(config.excludes(&spec_root, &root.join("specs/drafts/d.spec")));
        assert!(config.excludes(&spec_root, &root.join("specs/target/t.spec")));
        assert!(!config.excludes(&spec_root, &root.join("specs/a.spec")));
        // Outside the spec root nothing is excluded: it is no source at all.
        assert!(!config.excludes(&spec_root, &root.join("fixtures/fx.spec")));
    }

    #[test]
    fn an_entry_names_an_extension_or_a_wasm_file() {
        use ExtensionEntry::{File, Named};
        let cases = [
            ("@specforge/product", Named("@specforge/product")),
            (" @acme/foo@1.2.0 ", Named("@acme/foo")),
            ("greet", Named("greet")),
            (
                "greet.wasm",
                File {
                    name: None,
                    path: "greet.wasm",
                },
            ),
            (
                "./ext/greet.wasm",
                File {
                    name: None,
                    path: "./ext/greet.wasm",
                },
            ),
            (
                "/a@b/greet.wasm",
                File {
                    name: None,
                    path: "/a@b/greet.wasm",
                },
            ),
            (
                "@sdk/greet = ext/greet.wasm",
                File {
                    name: Some("@sdk/greet"),
                    path: "ext/greet.wasm",
                },
            ),
            // Plan 12 §2.2, as P4 reads them today.
            ("@acme/tool@", Named("@acme/tool")), // I2
            // The requirement is split off at the last inner `@`, whatever it holds.
            ("@acme/tool@1.0.0/x", Named("@acme/tool")), // I7
            ("foo@/bar", Named("foo")),                  // I9
            // A name that is no package name is still a named entry: the load
            // refuses it (E072), the reading does not.
            ("@acme/..", Named("@acme/..")),                   // I12
            ("../../../outside1", Named("../../../outside1")), // I23
        ];
        for (entry, expected) in cases {
            assert_eq!(ExtensionEntry::parse(entry), expected, "{entry}");
        }
    }

    #[test]
    fn a_named_entry_names_a_package_or_says_why_not() {
        let name = |entry: &str| ExtensionEntry::parse(entry).package_name();
        assert_eq!(
            name("@acme/tool@1.2.0").unwrap().unwrap().as_str(),
            "@acme/tool"
        );
        assert_eq!(name("greet").unwrap().unwrap().as_str(), "greet");
        for refused in ["../../../outside1", "@acme/..", "Bad Name"] {
            assert!(name(refused).unwrap().is_err(), "{refused}");
        }
        // A file entry names a file, not a package.
        assert!(name("ext/greet.wasm").is_none());
    }

    #[test]
    fn a_file_entry_resolves_against_the_root_and_names_what_it_writes() {
        let root = Path::new("/p");
        let bare = ExtensionEntry::parse("ext/greet.wasm");
        assert_eq!(bare.file(root), Some(PathBuf::from("/p/ext/greet.wasm")));
        assert_eq!(bare.name(), None);
        assert_eq!(extension_entry_name("ext/greet.wasm"), "ext/greet.wasm");

        let named = ExtensionEntry::parse("@sdk/greet=/abs/greet.wasm");
        assert_eq!(named.file(root), Some(PathBuf::from("/abs/greet.wasm")));
        assert_eq!(named.name(), Some("@sdk/greet"));
        assert_eq!(
            extension_entry_name("@sdk/greet=/abs/greet.wasm"),
            "@sdk/greet"
        );

        assert_eq!(ExtensionEntry::parse("@acme/foo@1.0.0").file(root), None);
    }

    /// `specforge.json` in a fresh temp directory, written as `text`.
    fn config_dir(text: &[u8]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), text).unwrap();
        dir
    }

    #[test]
    fn a_missing_config_is_the_default_with_no_problem() {
        let dir = tempfile::tempdir().unwrap();
        let read = read_project_config(dir.path());
        assert!(!read.found);
        assert!(read.problems.is_empty());
        assert!(read.config.raw.is_none());
        assert!(read.config.extensions.is_empty());
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_manifests",
        verify = "E069 names why specforge.json can't be used"
    )]
    fn an_unparsable_config_names_the_line_and_column() {
        let dir = config_dir(br#"{ "extensions": ["@specforge/product",  }"#);
        let path = dir.path().join("specforge.json");

        let read = read_project_config(dir.path());

        assert!(read.found);
        assert!(read.config.extensions.is_empty());
        assert!(read.config.raw.is_none());
        let [problem] = read.problems.as_slice() else {
            panic!("{:?}", read.problems);
        };
        assert!(
            matches!(problem, ConfigProblem::NotJson { .. }),
            "{problem:?}"
        );
        assert_eq!(
            problem.to_string(),
            format!(
                "{} is not valid JSON: expected value at line 1 column 41",
                path.display()
            )
        );
        assert!(problem.loads_nothing() && problem.blocks_edits());

        // Not readable (not UTF-8): the file is there, and nothing in it is used.
        let dir = config_dir(&[0xff, 0xfe, 0x7b]);
        let read = read_project_config(dir.path());
        assert!(read.found);
        let [problem] = read.problems.as_slice() else {
            panic!("{:?}", read.problems);
        };
        assert!(
            matches!(problem, ConfigProblem::Unreadable { .. }),
            "{problem:?}"
        );
        assert!(
            problem.to_string().starts_with(&format!(
                "failed to read {}: ",
                dir.path().join("specforge.json").display()
            )),
            "{problem}"
        );
        assert!(problem.loads_nothing());
    }

    #[test]
    fn a_config_that_is_not_an_object_is_a_problem() {
        let dir = config_dir(b"[1,2]");
        let read = read_project_config(dir.path());
        assert_eq!(
            read.problems,
            [ConfigProblem::NotAnObject {
                path: dir.path().join("specforge.json")
            }]
        );
        assert_eq!(
            read.problems[0].to_string(),
            format!(
                "{} must be a JSON object",
                dir.path().join("specforge.json").display()
            )
        );
        assert!(read.config.raw.is_none());
        assert!(read.problems[0].loads_nothing());
    }

    #[test]
    fn a_non_array_extensions_keeps_the_other_keys() {
        let dir = config_dir(br#"{"name":"p","spec_root":"spec","extensions":"x"}"#);
        let read = read_project_config(dir.path());
        assert_eq!(
            read.problems,
            [ConfigProblem::WrongType {
                path: dir.path().join("specforge.json"),
                key: "extensions",
                expected: "an array",
            }]
        );
        assert!(read.problems[0].loads_nothing());
        assert!(read.problems[0].blocks_edits());
        assert_eq!(
            read.problems[0].to_string(),
            format!(
                "\"extensions\" in {} must be an array",
                dir.path().join("specforge.json").display()
            )
        );
        assert_eq!(read.config.spec_root.as_deref(), Some("spec"));
        assert_eq!(read.config.name.as_deref(), Some("p"));
        assert!(read.config.extensions.is_empty());
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_manifests",
        verify = "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
    )]
    fn a_mistyped_key_is_a_problem_and_its_default_is_used() {
        let dir =
            config_dir(br#"{"spec_root": 5, "extensions": [], "name": "p", "version": null}"#);
        let path = dir.path().join("specforge.json");

        let read = read_project_config(dir.path());

        // In file order.
        assert_eq!(
            read.problems,
            [
                ConfigProblem::WrongType {
                    path: path.clone(),
                    key: "spec_root",
                    expected: "a string",
                },
                ConfigProblem::WrongType {
                    path: path.clone(),
                    key: "version",
                    expected: "a string",
                },
            ]
        );
        assert!(read.problems.iter().all(|p| !p.loads_nothing()));
        assert!(read.problems.iter().all(|p| !p.blocks_edits()));
        assert_eq!(read.config.spec_root, None);
        assert_eq!(read.config.version, None);
        assert_eq!(read.config.name.as_deref(), Some("p"));
    }

    #[specforge_test_macros::test(
        behavior = "load_extension_manifests",
        verify = "E069 names a mistyped key or a non-string item, and the rest of specforge.json is used"
    )]
    fn a_non_string_item_is_a_problem_and_the_others_are_kept() {
        let dir =
            config_dir(br#"{"extensions": ["@specforge/product", 42], "exclude": ["a", {}]}"#);
        let path = dir.path().join("specforge.json");

        let read = read_project_config(dir.path());

        assert_eq!(
            read.problems,
            [
                ConfigProblem::ItemNotAString {
                    path: path.clone(),
                    key: "extensions",
                    index: 1,
                    json: "42".into(),
                },
                ConfigProblem::ItemNotAString {
                    path: path.clone(),
                    key: "exclude",
                    index: 1,
                    json: "{}".into(),
                },
            ]
        );
        assert_eq!(
            read.problems[0].to_string(),
            format!(
                "\"extensions\"[1] in {} must be a string, found 42",
                path.display()
            )
        );
        assert!(read.problems.iter().all(|p| !p.blocks_edits()));
        assert_eq!(read.config.extensions, ["@specforge/product"]);
        assert_eq!(read.config.exclude, ["a"]);
        assert_eq!(
            load_project_config(dir.path()).extensions,
            ["@specforge/product"]
        );
    }

    #[test]
    fn keys_are_read_in_the_order_the_file_writes_them() {
        assert_eq!(
            keys_in_file_order(r#"{"z": 1, "a": {"nested": 2}, "m": [], "a": 3}"#),
            ["z", "a", "m"]
        );
        assert!(keys_in_file_order("[1]").is_empty());
    }
}
