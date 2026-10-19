//! The lookup of a format configuration (ADR 0021 D2, ADR 0169): from a document's directory up to its
//! project root, through the disk the caller hands `Configs`.

use super::*;
use specforge_formatter::FormatConfig;
use specforge_graph::FieldForms;
use specforge_registry::{Asked, MemoryDisk};

fn write_file(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
}

/// The configuration of a document in `dir`, in the project at `root`, and what reading it reported.
fn load(dir: &Path, root: &Path) -> (FormatConfig, Vec<Diagnostic>) {
    let document = document(
        Place::InProject { root, dir },
        "",
        None,
        None,
        &FieldForms::default(),
    );
    (document.config, document.diagnostics)
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "config file in project root is loaded"
)]
fn a_configuration_at_the_project_root_is_loaded() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write_file(root, "specforge.json", "{}");
    write_file(
        root,
        ".specforgefmt.toml",
        "indent_width = 4\nmax_width = 80\n",
    );

    let (config, diags) = load(root, root);

    assert!(diags.is_empty());
    assert_eq!(config.indent_width, 4);
    assert_eq!(config.max_width, 80);
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "config file in parent directory is discovered"
)]
fn a_configuration_in_a_parent_directory_is_found() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write_file(root, "specforge.json", "{}");
    write_file(root, ".specforgefmt.toml", "indent_width = 3\n");
    let subdir = root.join("spec").join("behaviors");
    std::fs::create_dir_all(&subdir).unwrap();

    let (config, diags) = load(&subdir, root);

    assert!(diags.is_empty());
    assert_eq!(config.indent_width, 3);
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "config outside project root is not discovered"
)]
fn a_configuration_above_the_project_root_is_not_found() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    // The configuration is in tmp root, but the project root is a subdirectory.
    write_file(root, ".specforgefmt.toml", "indent_width = 8\n");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_file(&project, "specforge.json", "{}");

    let (config, diags) = load(&project, &project);

    assert!(diags.is_empty());
    assert_eq!(config.indent_width, 2, "the defaults");
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "invalid indent_width produces diagnostic and uses default"
)]
fn an_invalid_indent_width_is_a_w141_and_the_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write_file(root, "specforge.json", "{}");
    write_file(
        root,
        ".specforgefmt.toml",
        "indent_width = \"not_a_number\"\n",
    );

    let (config, diags) = load(root, root);

    assert_eq!(config.indent_width, 2);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "W141");
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "missing config file uses defaults"
)]
fn a_project_without_a_configuration_uses_the_defaults() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write_file(root, "specforge.json", "{}");

    let (config, diags) = load(root, root);

    assert!(diags.is_empty());
    assert_eq!(config, FormatConfig::default());
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "Load Format Configuration: format config loading holds — project_root_available, filesystem_accessible, config_resolved, walk_bounded, invalid_values_diagnosed"
)]
fn the_load_format_config_contract_holds() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    // requires: project_root_available
    write_file(root, "specforge.json", "{}");
    write_file(
        root,
        ".specforgefmt.toml",
        "indent_width = 4\nmax_width = 80\n",
    );
    let subdir = root.join("spec").join("behaviors");
    std::fs::create_dir_all(&subdir).unwrap();

    // ensures: config_resolved
    let (config, diags) = load(&subdir, root);
    assert_eq!(config.indent_width, 4, "config should be resolved");
    assert_eq!(config.max_width, 80);
    assert!(
        diags.is_empty(),
        "valid config should produce no diagnostics"
    );

    // ensures: walk_bounded (no configuration in the project: the defaults)
    let isolated = tempfile::TempDir::new().unwrap();
    write_file(isolated.path(), "specforge.json", "{}");
    let (default_config, default_diags) = load(isolated.path(), isolated.path());
    assert_eq!(default_config, FormatConfig::default());
    assert!(default_diags.is_empty());

    // ensures: invalid_values_diagnosed
    let invalid = tempfile::TempDir::new().unwrap();
    write_file(invalid.path(), "specforge.json", "{}");
    write_file(invalid.path(), ".specforgefmt.toml", "indent_width = 999\n");
    let (inv_config, inv_diags) = load(invalid.path(), invalid.path());
    assert_eq!(inv_config.indent_width, 2, "falls back to the default");
    assert!(!inv_diags.is_empty(), "an invalid value is diagnosed");
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "config discovery walks from formatted file directory up to specforge.json parent then stops"
)]
fn the_lookup_walks_from_the_files_directory_up_to_its_project_root_then_stops() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write_file(root, "specforge.json", "{}");
    write_file(root, ".specforgefmt.toml", "indent_width = 3\n");
    let deep = root.join("spec").join("behaviors").join("auth");
    std::fs::create_dir_all(&deep).unwrap();

    let (config, _) = load(&deep, root);
    assert_eq!(config.indent_width, 3, "found by walking up");

    // A configuration above the project root is not found.
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_file(&project, "specforge.json", "{}");
    let (above, _) = load(&project, &project);
    assert_eq!(above, FormatConfig::default());
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "a configuration file is read once per lookup, whichever directories share it"
)]
fn a_configuration_file_is_read_once_per_lookup_whichever_directories_share_it() {
    let file = "/specforge-memory/p/.specforgefmt.toml";
    let disk = MemoryDisk::new().with_file(file, b"indent_width = 4\n");
    let mut configs = Configs::new(None, &disk);
    let root = Path::new("/specforge-memory/p");

    for dir in ["spec/a", "spec/b", "spec/a"] {
        let place = Place::InProject {
            root,
            dir: &root.join(dir),
        };
        assert_eq!(configs.layout(place).indent_width, 4);
    }

    let reads = disk
        .asked()
        .into_iter()
        .filter(|asked| *asked == Asked::Read(PathBuf::from(file)))
        .count();
    assert_eq!(reads, 1, "{:?}", disk.asked());
}

#[specforge_test(
    behavior = "load_format_config",
    verify = "a configuration the disk cannot read is a W141 naming why, and its defaults apply"
)]
fn a_configuration_the_disk_cannot_read_is_a_w141_and_the_walk_stops() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write_file(root, "specforge.json", "{}");
    write_file(root, ".specforgefmt.toml", "indent_width = 4\n");
    // A directory where the nearest configuration should be: it cannot be read.
    let spec = root.join("spec");
    std::fs::create_dir_all(spec.join(".specforgefmt.toml")).unwrap();

    let (config, diags) = load(&spec, root);

    assert_eq!(config.indent_width, 2, "the defaults, not the root's 4");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].message.starts_with("Failed to read config file"),
        "{diags:?}"
    );
}
