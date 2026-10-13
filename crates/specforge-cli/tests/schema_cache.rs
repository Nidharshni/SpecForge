// `specforge export` keeps `.specforge/schema-cache.json`: it compares the
// schema it generates against the cache the previous export wrote, warns
// W053 for each breaking change, and after a successful export replaces the
// cache atomically. `check` and `watch` write no cache.

use crate::e2e_fixtures::*;
use serde_json::{Value, json};
use specforge_test::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SOFTWARE_ONLY: &str =
    r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software"]}"#;
const SOFTWARE_AND_GOVERNANCE: &str = r#"{"name":"t","version":"0.1.0","spec_root":"spec","extensions":["@specforge/software","@specforge/governance"]}"#;

fn software_project() -> TempDir {
    setup_project_with_config(SOFTWARE_ONLY, &[("main.spec", SOFTWARE_SPEC)])
}

fn cache_path(dir: &TempDir) -> PathBuf {
    dir.path().join(".specforge").join("schema-cache.json")
}

fn export(dir: &TempDir, args: &[&str]) -> Output {
    specforge_cmd()
        .arg("export")
        .args(args)
        .arg(dir.path())
        .output()
        .unwrap()
}

/// Run a successful `specforge export` and return its stderr.
fn export_ok(dir: &TempDir) -> String {
    let output = export(dir, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "export failed: {stderr}");
    stderr
}

fn read_cache(dir: &TempDir) -> Value {
    let text = fs::read_to_string(cache_path(dir))
        .unwrap_or_else(|e| panic!("no schema cache at {}: {e}", cache_path(dir).display()));
    serde_json::from_str(&text).unwrap()
}

fn write_cache(dir: &TempDir, cache: &Value) {
    fs::write(
        cache_path(dir),
        serde_json::to_string_pretty(cache).unwrap(),
    )
    .unwrap();
}

/// The schema `specforge schema` generates for the project.
fn generated_schema(dir: &TempDir) -> Value {
    let output = specforge_cmd()
        .arg("schema")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    parse_json_stdout(&output)
}

fn kind_mut<'a>(cache: &'a mut Value, name: &str) -> &'a mut Value {
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|k| k["name"] == name)
        .unwrap_or_else(|| panic!("no kind {name} in the cache"))
}

fn w053_lines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|l| l.starts_with("warning[W053]"))
        .collect()
}

/// Export once, let `edit` change the cache the export wrote (standing in
/// for the schema an older extension set produced), export again and
/// return the second export's stderr.
fn export_against_edited_cache(edit: impl FnOnce(&mut Value)) -> String {
    let dir = software_project();
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    edit(&mut cache);
    write_cache(&dir, &cache);
    export_ok(&dir)
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// --- The cache is written by export ---

#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "schema-cache.json written after schema generation"
)]
fn first_export_creates_the_schema_cache() {
    let dir = software_project();
    assert!(!cache_path(&dir).exists());

    let stderr = export_ok(&dir);

    let cache = read_cache(&dir);
    // The cache holds the schema the export generated, with its hash.
    assert_eq!(cache["schema"], generated_schema(&dir));
    let hash = cache["content_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 64, "a sha256 hex digest: {hash}");
    // No previous schema: nothing is breaking and nothing is reported.
    assert!(!stderr.contains("W053"), "{stderr}");
    assert!(!stderr.contains("I016"), "{stderr}");
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "no previous schema treats all changes as non-breaking"
)]
fn first_export_of_a_project_reports_nothing() {
    // Every kind and edge type is new against no previous schema; none of
    // that is breaking, and a project never exported gets no I016.
    let dir = software_project();
    let stderr = export_ok(&dir);
    assert!(w053_lines(&stderr).is_empty(), "{stderr}");
    assert!(!stderr.contains("I016"), "{stderr}");
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "missing cache without prior exports emits no diagnostic"
)]
fn export_without_a_cache_emits_no_diagnostic() {
    let dir = software_project();
    let stderr = export_ok(&dir);
    assert_eq!(stderr, "", "a first export writes nothing to stderr");
}

#[test]
fn an_unchanged_schema_reports_nothing_on_the_next_export() {
    let dir = software_project();
    export_ok(&dir);
    let first = fs::read_to_string(cache_path(&dir)).unwrap();

    let stderr = export_ok(&dir);

    assert_eq!(stderr, "", "an unchanged schema reports nothing");
    assert_eq!(fs::read_to_string(cache_path(&dir)).unwrap(), first);
}

#[test]
fn every_export_format_updates_the_cache() {
    for args in [
        &["--format=brief"][..],
        &["--format=context"],
        &["--format=dot"],
        &["--no-schema"],
    ] {
        let dir = software_project();
        let output = export(&dir, args);
        assert!(output.status.success(), "{args:?}");
        assert_eq!(
            read_cache(&dir)["schema"],
            generated_schema(&dir),
            "{args:?} must persist the generated schema"
        );
    }
}

#[test]
fn schema_version_negotiation_does_not_change_the_cached_schema() {
    // --schema-version relabels the embedded schema; the cache keeps the
    // schema the extensions produce.
    let dir = software_project();
    let output = export(&dir, &["--schema-version", "1.0.0"]);
    assert!(output.status.success());
    assert_eq!(read_cache(&dir)["schema"], generated_schema(&dir));
}

#[test]
fn a_failed_export_leaves_the_cache_alone() {
    let dir = software_project();
    let output = export(&dir, &["--scope", "no_such_entity"]);
    assert!(!output.status.success());
    assert!(
        !cache_path(&dir).exists(),
        "no cache without a successful export"
    );

    // With a cache from an earlier export, a failed one doesn't replace it.
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}));
    write_cache(&dir, &cache);
    let output = export(&dir, &["--scope", "no_such_entity"]);
    assert!(!output.status.success());
    assert_eq!(read_cache(&dir), cache);
}

#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "cache file overwritten atomically via temp+rename"
)]
fn export_replaces_the_cache_atomically() {
    let dir = software_project();
    export_ok(&dir);
    let mut stale = read_cache(&dir);
    stale["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}));
    write_cache(&dir, &stale);
    let stale_text = fs::read_to_string(cache_path(&dir)).unwrap();

    // A hard link shares the cache file's inode. Rewriting the file in
    // place would change what the link reads; a rename puts a new inode at
    // the path and leaves the link on the old, complete content.
    let cache_dir = dir.path().join(".specforge");
    let link = cache_dir.join("previous-cache.json");
    fs::hard_link(cache_path(&dir), &link).unwrap();

    export_ok(&dir);

    assert_eq!(fs::read_to_string(&link).unwrap(), stale_text);
    assert_eq!(read_cache(&dir)["schema"], generated_schema(&dir));
    // The temp file was renamed away: no leftover beside the cache.
    assert_eq!(
        names_in(&cache_dir),
        vec!["previous-cache.json", "schema-cache.json"]
    );
}

// --- The cache feeds breaking-change detection ---

#[specforge_test(
    behavior = "persist_schema_cache",
    verify = "persisted cache feeds breaking change detection in the next compilation"
)]
fn removing_an_extension_warns_on_the_next_export() {
    let dir = setup_project_with_config(SOFTWARE_AND_GOVERNANCE, &[("main.spec", SOFTWARE_SPEC)]);
    assert_eq!(export_ok(&dir), "");

    // The upgrade drops governance: its kinds leave the schema.
    fs::write(dir.path().join("specforge.json"), SOFTWARE_ONLY).unwrap();
    let stderr = export_ok(&dir);

    let warnings = w053_lines(&stderr);
    assert!(
        warnings
            .iter()
            .any(|l| l.contains("entity kind `decision` was removed")),
        "{stderr}"
    );
    // The export still went out, and the cache now holds the new schema,
    // so the same schema warns no more.
    assert_eq!(read_cache(&dir)["schema"], generated_schema(&dir));
    assert_eq!(export_ok(&dir), "");
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "previous schema is read from .specforge/schema-cache.json"
)]
fn the_previous_schema_comes_from_the_cache_file() {
    let stderr =
        export_against_edited_cache(|cache| {
            cache["schema"]["entity_kinds"].as_array_mut().unwrap().push(
            json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}),
        );
        });
    assert_eq!(
        w053_lines(&stderr),
        vec![
            "warning[W053]: breaking schema change since the last export: entity kind `legacy` was removed"
        ],
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed entity kind detected as breaking"
)]
fn a_removed_kind_warns() {
    let stderr =
        export_against_edited_cache(|cache| {
            cache["schema"]["entity_kinds"].as_array_mut().unwrap().push(
            json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}),
        );
        });
    assert!(
        stderr.contains("entity kind `legacy` was removed"),
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed field is breaking"
)]
fn a_removed_field_warns() {
    let stderr = export_against_edited_cache(|cache| {
        kind_mut(cache, "behavior")["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "name": "legacy_note",
                "field_type": "string",
                "required": false,
                "source_extension": "@specforge/software"
            }));
    });
    assert_eq!(
        w053_lines(&stderr),
        vec![
            "warning[W053]: breaking schema change since the last export: field `legacy_note` was removed from `behavior`"
        ],
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new required field detected as breaking"
)]
fn a_new_required_field_warns() {
    // The previous schema had no `contract` on behavior; it is required now.
    let stderr = export_against_edited_cache(|cache| {
        let fields = kind_mut(cache, "behavior")["fields"]
            .as_array_mut()
            .unwrap();
        let before = fields.len();
        fields.retain(|f| !(f["name"] == "contract" && f["required"] == true));
        assert_eq!(fields.len(), before - 1, "behavior.contract is required");
    });
    assert_eq!(
        w053_lines(&stderr),
        vec![
            "warning[W053]: breaking schema change since the last export: required field `contract` was added to `behavior`"
        ],
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "added optional field detected as non-breaking"
)]
fn a_new_optional_field_is_silent() {
    let stderr = export_against_edited_cache(|cache| {
        let fields = kind_mut(cache, "behavior")["fields"]
            .as_array_mut()
            .unwrap();
        let optional = fields
            .iter()
            .position(|f| f["required"] == false)
            .expect("behavior has an optional field");
        fields.remove(optional);
    });
    assert_eq!(stderr, "", "an added optional field is not breaking");
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new entity kind detected as non-breaking"
)]
fn a_new_kind_is_silent() {
    let stderr = export_against_edited_cache(|cache| {
        cache["schema"]["entity_kinds"]
            .as_array_mut()
            .unwrap()
            .retain(|k| k["name"] != "port");
    });
    assert_eq!(stderr, "", "an added kind is not breaking");
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "removed edge type detected as breaking"
)]
fn a_removed_edge_type_warns() {
    let stderr = export_against_edited_cache(|cache| {
        cache["schema"]["edge_types"]
            .as_array_mut()
            .unwrap()
            .push(json!({"label": "legacy_edge", "source_extension": "x"}));
    });
    assert_eq!(
        w053_lines(&stderr),
        vec![
            "warning[W053]: breaking schema change since the last export: edge type `legacy_edge` was removed"
        ],
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "new edge type detected as non-breaking"
)]
fn a_new_edge_type_is_silent() {
    let stderr = export_against_edited_cache(|cache| {
        let edges = cache["schema"]["edge_types"].as_array_mut().unwrap();
        assert!(!edges.is_empty(), "software registers edge types");
        edges.remove(0);
    });
    assert_eq!(stderr, "", "an added edge type is not breaking");
}

// --- Read-only commands leave the cache alone ---

#[test]
fn check_writes_no_schema_cache() {
    let dir = software_project();
    let output = specforge_cmd()
        .arg("check")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!cache_path(&dir).exists(), "check must not write the cache");

    // Nor does it touch a cache an export wrote.
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    kind_mut(&mut cache, "behavior")["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "legacy_note", "field_type": "string", "required": false, "source_extension": "x"}));
    write_cache(&dir, &cache);
    let output = specforge_cmd()
        .arg("check")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(read_cache(&dir), cache);
}

#[test]
fn watch_writes_no_schema_cache() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let dir = software_project();
    let mut watch = std::process::Command::new(assert_cmd::cargo_bin!("specforge"));
    watch
        .args(["watch", "--format", "json", "--path"])
        .arg(dir.path());
    let mut child = crate::child_guard::ChildGuard::spawn(
        crate::child_guard::guarded_command(&watch)
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let stdout = child.take_stdout().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let wait_for = |needle: &str| {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if let Ok(line) = rx.recv_timeout(Duration::from_secs(1))
                && line.contains(needle)
            {
                return true;
            }
        }
        false
    };

    let ready = wait_for("\"event\":\"ready\"");
    std::thread::sleep(Duration::from_millis(300));
    fs::write(
        dir.path().join("spec").join("main.spec"),
        format!("{SOFTWARE_SPEC}\ninvariant extra \"Extra\" {{ guarantee \"x\" }}\n"),
    )
    .unwrap();
    let rebuilt = wait_for("\"event\":\"rebuilt\"");
    drop(child);

    assert!(ready, "watch never reported ready");
    assert!(rebuilt, "watch never rebuilt");
    assert!(!cache_path(&dir).exists(), "watch must not write the cache");
}

// --- The export carries the computed schema version ---

fn version(major: u64, minor: u64, patch: u64) -> Value {
    json!({"major": major, "minor": minor, "patch": patch})
}

/// The schema version a `specforge export` embeds, from its stdout.
fn exported_version(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let export = parse_json_stdout(output);
    let embedded = export["schema"]["schema_version"].clone();
    assert_eq!(
        export["schema_version"],
        format!(
            "{}.{}.{}",
            embedded["major"], embedded["minor"], embedded["patch"]
        ),
        "the envelope names the embedded schema's version"
    );
    embedded
}

/// Export once, let `edit` change the cache the export wrote, export again
/// and return the project, the version the second export embedded and the
/// one it cached.
fn version_after_edited_cache(edit: impl FnOnce(&mut Value)) -> (TempDir, Value, Value) {
    let dir = software_project();
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    edit(&mut cache);
    write_cache(&dir, &cache);
    let embedded = exported_version(&export(&dir, &[]));
    let cached = read_cache(&dir)["schema"]["schema_version"].clone();
    (dir, embedded, cached)
}

fn add_legacy_kind(cache: &mut Value) {
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "legacy", "source_extension": "x", "testable": false, "fields": []}));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "first compilation without cache produces version 1.0.0"
)]
fn a_first_export_is_schema_version_1_0_0() {
    let dir = software_project();
    assert_eq!(exported_version(&export(&dir, &[])), version(1, 0, 0));
    assert_eq!(
        read_cache(&dir)["schema"]["schema_version"],
        version(1, 0, 0)
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "removed entity kind triggers major version bump"
)]
fn a_removed_kind_bumps_the_exported_major_version() {
    let (_dir, embedded, cached) = version_after_edited_cache(|cache| {
        cache["schema"]["schema_version"] = version(1, 2, 3);
        add_legacy_kind(cache);
    });
    assert_eq!(embedded, version(2, 0, 0));
    // The cache keeps the bumped version, so the next export builds on it.
    assert_eq!(cached, version(2, 0, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "new entity kind triggers minor version bump"
)]
fn a_new_kind_bumps_the_exported_minor_version() {
    let (_dir, embedded, cached) = version_after_edited_cache(|cache| {
        cache["schema"]["schema_version"] = version(1, 2, 3);
        cache["schema"]["entity_kinds"]
            .as_array_mut()
            .unwrap()
            .retain(|k| k["name"] != "port");
    });
    assert_eq!(embedded, version(1, 3, 0));
    assert_eq!(cached, version(1, 3, 0));
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "no changes returns previous"
)]
fn an_unchanged_schema_keeps_the_exported_version() {
    let (_dir, embedded, cached) = version_after_edited_cache(|cache| {
        cache["schema"]["schema_version"] = version(3, 1, 4);
    });
    assert_eq!(embedded, version(3, 1, 4));
    assert_eq!(cached, version(3, 1, 4));
}

#[test]
fn the_schema_command_reports_the_version_export_embeds() {
    let (dir, embedded, _) = version_after_edited_cache(|cache| {
        cache["schema"]["schema_version"] = version(1, 2, 3);
        add_legacy_kind(cache);
    });
    assert_eq!(generated_schema(&dir)["schema_version"], embedded);
    // `schema` only reads the cache.
    assert_eq!(generated_schema(&dir)["schema_version"], embedded);
}

// --- One schema version names one document ---

fn field_mut<'a>(cache: &'a mut Value, kind: &str, field: &str) -> &'a mut Value {
    kind_mut(cache, kind)["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["name"] == field)
        .unwrap_or_else(|| panic!("no field {kind}.{field} in the cache"))
}

fn edge_mut<'a>(cache: &'a mut Value, label: &str) -> &'a mut Value {
    cache["schema"]["edge_types"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["label"] == label)
        .unwrap_or_else(|| panic!("no edge type {label} in the cache"))
}

fn export_version_string(output: &Output) -> String {
    assert!(
        output.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse_json_stdout(output)["schema_version"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "a field's required flag changing either way is breaking"
)]
#[ignore = "fixed by T3"]
fn a_changed_required_flag_bumps_the_major_version_and_warns() {
    let dir = software_project();
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    field_mut(&mut cache, "behavior", "contract")["required"] = json!(false);
    write_cache(&dir, &cache);

    let output = export(&dir, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert_eq!(exported_version(&output), version(2, 0, 0));
    assert_eq!(
        read_cache(&dir)["schema"]["schema_version"],
        version(2, 0, 0)
    );
    assert_eq!(
        w053_lines(&stderr),
        vec![
            "warning[W053]: breaking schema change since the last export: the required flag of field `contract` of `behavior` changed from false to true"
        ],
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "an edge type losing a source or target kind is breaking; gaining one is an addition"
)]
#[ignore = "fixed by T3"]
fn an_edge_type_that_loses_a_target_kind_bumps_the_major_version() {
    let dir = software_project();
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    edge_mut(&mut cache, "BehaviorEnforcesInvariant")["target_kinds"] =
        json!(["feature", "invariant"]);
    write_cache(&dir, &cache);

    let output = export(&dir, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert_eq!(exported_version(&output), version(2, 0, 0));
    assert_eq!(
        w053_lines(&stderr),
        vec![
            "warning[W053]: breaking schema change since the last export: the target kinds of edge type `BehaviorEnforcesInvariant` lost `feature`"
        ],
        "{stderr}"
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "an extension's version changing is a patch"
)]
#[ignore = "fixed by T3"]
fn an_extension_version_change_bumps_the_patch_version() {
    let dir = software_project();
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    cache["schema"]["extensions"][0]["version"] = json!("0.9.0");
    write_cache(&dir, &cache);

    let output = export(&dir, &[]);

    assert_eq!(exported_version(&output), version(1, 0, 1));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[specforge_test(
    behavior = "negotiate_schema_version",
    verify = "a requested version of the same major, not newer, is admitted and the export carries the current version"
)]
#[ignore = "fixed by T4"]
fn a_requested_version_does_not_relabel_the_export() {
    let dir = software_project();
    export_ok(&dir);
    let mut cache = read_cache(&dir);
    cache["schema"]["entity_kinds"]
        .as_array_mut()
        .unwrap()
        .retain(|k| k["name"] != "port");
    write_cache(&dir, &cache);
    assert_eq!(export_version_string(&export(&dir, &[])), "1.1.0");

    let scoped = export(
        &dir,
        &["--scope", "parse_input", "--schema-version", "1.0.0"],
    );

    assert!(scoped.status.success(), "{scoped:?}");
    let document = parse_json_stdout(&scoped);
    assert_eq!(document["schema_version"], "1.1.0");
    assert!(
        document["schema_ref"]["url"]
            .as_str()
            .unwrap()
            .ends_with("graph-protocol-v1.1.0.json"),
        "{}",
        document["schema_ref"]
    );
    assert_eq!(
        document["schema_ref"]["content_hash"],
        read_cache(&dir)["content_hash"]
    );
}

#[specforge_test(
    behavior = "detect_breaking_schema_changes",
    verify = "an unreadable schema cache restarts the version at 1.0.0 with I016"
)]
#[ignore = "fixed by T3"]
fn an_unreadable_cache_restarts_the_version_with_i016() {
    let dir = software_project();
    export_ok(&dir);
    fs::write(cache_path(&dir), r#"{"schema": {"#).unwrap();

    let output = export(&dir, &[]);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert_eq!(exported_version(&output), version(1, 0, 0));
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("info[I016]: the schema cache "))
        .collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    assert!(lines[0].contains("could not be read"), "{stderr}");
    assert!(lines[0].contains("starts again at 1.0.0"), "{stderr}");
    // The export recorded a new cache.
    assert_eq!(
        read_cache(&dir)["schema"]["schema_version"],
        version(1, 0, 0)
    );
}

#[specforge_test(
    behavior = "compute_schema_version",
    verify = "one schema version names one document: equal versions have equal content hashes"
)]
fn an_unchanged_schema_keeps_its_version_and_its_hash() {
    let dir = software_project();
    export_ok(&dir);
    let first = read_cache(&dir);

    let stderr = export_ok(&dir);

    let second = read_cache(&dir);
    assert_eq!(first["content_hash"], second["content_hash"]);
    assert_eq!(
        first["schema"]["schema_version"],
        second["schema"]["schema_version"]
    );
    assert_eq!(stderr, "");
}
