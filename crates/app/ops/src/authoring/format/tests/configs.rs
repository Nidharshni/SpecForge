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

/// A project (`specforge.json` = `{}`, `spec/a.spec` = `MESSY`) and a directory beside it that is outside.
#[cfg(unix)]
fn scene() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = project_with("{}", &[("spec/a.spec", MESSY)]);
    let proj = dir.path().join("proj");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::rename(
        dir.path().join("specforge.json"),
        proj.join("specforge.json"),
    )
    .unwrap();
    std::fs::rename(dir.path().join("spec"), proj.join("spec")).unwrap();
    (dir, proj, outside)
}

/// A check run over `proj` under `caller`; what the file `spec/a.spec` would become, and the run's W141s.
#[cfg(unix)]
fn check_a(proj: &Path, caller: crate::Caller<'_>) -> (String, Vec<Diagnostic>) {
    let outcome = super::super::run(&Request {
        root: &ProjectRoot::of(proj).unwrap(),
        paths: &[],
        mode: Mode::Check,
        caller,
        runtime: None,
    })
    .unwrap();
    let w141 = outcome
        .diagnostics
        .iter()
        .filter(|d| d.is(codes::W141))
        .cloned()
        .collect();
    (outcome.changes[0].after.clone(), w141)
}

#[cfg(unix)]
#[specforge_test(
    behavior = "load_format_config",
    verify = "a bounded caller's configuration that leaves the bound or has more than one name is a W141 naming where it lands or how many names it has, never its content"
)]
fn a_bounded_run_reports_a_configuration_linked_outside_naming_where_it_lands() {
    let (_scene, proj, outside) = scene();
    std::fs::write(outside.join("c.toml"), "indent_width = 4\nSECRET = = x\n").unwrap();
    std::os::unix::fs::symlink(outside.join("c.toml"), proj.join(".specforgefmt.toml")).unwrap();
    let held = crate::Containment::within(&proj);

    let (after, w141) = check_a(&proj, crate::testing::bounded(&held));

    assert_eq!(w141.len(), 1, "{w141:?}");
    let message = &w141[0].message;
    assert!(
        message.contains(
            outside
                .join("c.toml")
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
        ),
        "{message}"
    );
    assert!(message.contains("outside"), "{message}");
    assert!(!message.contains("SECRET"), "{message}");
    assert!(after.contains("\n  contract"), "the defaults: {after}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "load_format_config",
    verify = "a bounded caller's configuration that leaves the bound or has more than one name is a W141 naming where it lands or how many names it has, never its content"
)]
fn a_bounded_run_reports_a_hard_linked_configuration_by_its_count() {
    let (_scene, proj, outside) = scene();
    std::fs::write(outside.join("c.toml"), "indent_width = 4\nSECRET = = x\n").unwrap();
    std::fs::hard_link(outside.join("c.toml"), proj.join(".specforgefmt.toml")).unwrap();
    let held = crate::Containment::within(&proj);

    let (after, w141) = check_a(&proj, crate::testing::bounded(&held));

    assert_eq!(w141.len(), 1, "{w141:?}");
    assert!(w141[0].message.contains("2 names"), "{w141:?}");
    assert!(!w141[0].message.contains("SECRET"), "{w141:?}");
    assert!(after.contains("\n  contract"), "the defaults: {after}");
}

#[cfg(unix)]
#[specforge_test(
    behavior = "load_format_config",
    verify = "an unbounded caller follows a linked configuration"
)]
fn an_unbounded_run_follows_a_linked_configuration() {
    let (_scene, proj, outside) = scene();
    std::fs::write(outside.join("c.toml"), "indent_width = 4\nSECRET = = x\n").unwrap();
    std::os::unix::fs::symlink(outside.join("c.toml"), proj.join(".specforgefmt.toml")).unwrap();

    let (_, w141) = check_a(&proj, crate::testing::caller());

    // The CLI follows the link (ADR 0092 D6): the file is read, and its own TOML error is the W141.
    assert_eq!(w141.len(), 1, "{w141:?}");
    assert!(w141[0].message.contains("Invalid TOML"), "{w141:?}");
}
