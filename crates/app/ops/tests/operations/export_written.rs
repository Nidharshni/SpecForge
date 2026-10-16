//! An export written into a directory of the project: held through the caller's containment,
//! replaced atomically, refused about the directory (ADR 0131).

use crate::view_support::{Assembled, Viewed};
use serde_json::Value;
use specforge_common::ProjectPath;
use specforge_ops::asking::Asking;
use specforge_ops::read::export::{
    DIRECTORY_WITHOUT_PROJECT, Directory, EXPORT_WRITE_FAILED, ExportField, FORMAT, Format,
    NOT_A_DIRECTORY, Request, write_into,
};
use specforge_ops::view::ProjectView;
use specforge_ops::{Caller, Containment, OUTSIDE_ROOT, OpErrorKind, Refusal};
use std::path::Path;

fn project() -> Assembled {
    Assembled::over(crate::view_support::registries(&["behavior"], &[]))
        .source("behavior b \"B\" {\n  contract \"The system MUST work\"\n}\n")
}

fn request(format: Format) -> Request<'static> {
    Request {
        format: Some(format),
        ..Request::default()
    }
}

/// `name` written for `format`, with the project's root as the bound.
fn write(
    project: &Assembled,
    format: Format,
    name: &str,
) -> Result<ProjectPath, Refusal<ExportField>> {
    let containment = Containment::within(project.root());
    write_through(&project.view(), &containment, request(format), name)
}

fn write_through(
    view: &ProjectView,
    containment: &Containment,
    request: Request,
    name: &str,
) -> Result<ProjectPath, Refusal<ExportField>> {
    let caller = Caller::new(containment, Asking::Nobody);
    let directory = Directory {
        name: Path::new(name),
        caller,
    };
    write_into(view, &request, &directory).map(|written| written.into_value_and_notices().0)
}

fn refused(result: Result<ProjectPath, Refusal<ExportField>>) -> Refusal<ExportField> {
    result.expect_err("the call is refused")
}

#[test]
fn each_format_is_written_to_its_own_file() {
    let project = project();
    for choice in FORMAT.choices {
        let file = write(&project, choice.value, "out").unwrap();
        assert_eq!(file, format!("out/{}", choice.value.file_name()).as_str());
    }
    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(project.root().join("out/graph.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(graph["nodes"][0]["id"], "b", "{graph}");
    let dot = std::fs::read_to_string(project.root().join("out/graph.dot")).unwrap();
    assert!(dot.starts_with("digraph"), "{dot}");
}

#[test]
fn a_relative_directory_is_read_against_the_project_root() {
    let project = project();
    // Unbounded, as the CLI's: a relative name would be the process's directory if it were read so.
    let file = write_through(
        &project.view(),
        &Containment::unbounded(),
        request(Format::Brief),
        "rel/dir",
    )
    .unwrap();
    assert_eq!(file, "rel/dir/brief.json");
    assert!(project.root().join("rel/dir/brief.json").is_file());
}

#[test]
fn an_absolute_directory_inside_is_named_from_the_root() {
    let project = project();
    let absolute = project.root().join("abs");
    let file = write(&project, Format::Context, absolute.to_str().unwrap()).unwrap();
    assert_eq!(file, "abs/context.json");
}

/// What lies outside the project, to check that nothing was written there.
fn outside() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn assert_outside(refusal: &Refusal<ExportField>) {
    assert_eq!(refusal.subject, Some(ExportField::Directory));
    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, OUTSIDE_ROOT);
}

#[test]
fn a_directory_that_leaves_the_project_is_refused_about_the_directory() {
    let project = project();
    let outside = outside();
    assert_outside(&refused(write(&project, Format::Brief, "../x")));
    let absolute = outside.path().join("abs");
    assert_outside(&refused(write(
        &project,
        Format::Brief,
        absolute.to_str().unwrap(),
    )));
    assert!(!absolute.exists());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), project.root().join("linkout")).unwrap();
        assert_outside(&refused(write(&project, Format::Brief, "linkout")));
        // A dangling link at the file's name.
        std::fs::create_dir_all(project.root().join("dangle")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("missing.json"),
            project.root().join("dangle/brief.json"),
        )
        .unwrap();
        assert_outside(&refused(write(&project, Format::Brief, "dangle")));
    }
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn a_link_at_the_files_name_that_leaves_the_project_is_refused() {
    let project = project();
    let outside = outside();
    let victim = outside.path().join("victim.json");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::create_dir_all(project.root().join("fileout")).unwrap();
    std::os::unix::fs::symlink(&victim, project.root().join("fileout/graph.json")).unwrap();

    assert_outside(&refused(write(&project, Format::Graph, "fileout")));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
}

#[cfg(unix)]
#[test]
fn a_link_that_stays_inside_is_followed_and_named_as_spelled() {
    let project = project();
    std::fs::create_dir_all(project.root().join("out")).unwrap();
    std::os::unix::fs::symlink(project.root().join("out"), project.root().join("linkin")).unwrap();

    let file = write(&project, Format::Brief, "linkin").unwrap();

    assert_eq!(file, "linkin/brief.json");
    assert!(project.root().join("out/brief.json").is_file());
    assert!(
        std::fs::symlink_metadata(project.root().join("linkin"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn the_file_is_replaced_and_no_temporary_is_left() {
    let project = project();
    write(&project, Format::Brief, "out").unwrap();
    std::fs::write(project.root().join("out/brief.json"), "STALE").unwrap();
    write(&project, Format::Brief, "out").unwrap();
    let text = std::fs::read_to_string(project.root().join("out/brief.json")).unwrap();
    assert!(text.contains("\"nodes\""), "{text}");
    assert!(!project.root().join("out/brief.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn a_temporary_that_is_a_link_is_removed_and_its_target_is_untouched() {
    let project = project();
    let outside = outside();
    let victim = outside.path().join("victim.tmp");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::create_dir_all(project.root().join("out")).unwrap();
    std::os::unix::fs::symlink(&victim, project.root().join("out/brief.json.tmp")).unwrap();

    write(&project, Format::Brief, "out").unwrap();

    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    assert!(!project.root().join("out/brief.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn a_hard_link_at_the_files_name_keeps_its_other_name() {
    let project = project();
    let outside = outside();
    let victim = outside.path().join("victim.json");
    std::fs::write(&victim, "ORIGINAL").unwrap();
    std::fs::create_dir_all(project.root().join("hard")).unwrap();
    std::fs::hard_link(&victim, project.root().join("hard/brief.json")).unwrap();

    let file = write(&project, Format::Brief, "hard").unwrap();

    assert_eq!(file, "hard/brief.json");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "ORIGINAL");
    let text = std::fs::read_to_string(project.root().join("hard/brief.json")).unwrap();
    assert!(text.contains("\"nodes\""), "{text}");
}

#[cfg(unix)]
#[test]
fn a_directory_it_cannot_write_is_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let project = project();
    let rodir = project.root().join("rodir");
    std::fs::create_dir_all(&rodir).unwrap();
    std::fs::set_permissions(&rodir, std::fs::Permissions::from_mode(0o555)).unwrap();
    // A process that may write there anyway (root) cannot show the refusal.
    if std::fs::write(rodir.join("probe"), "x").is_ok() {
        std::fs::set_permissions(&rodir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let refusal = refused(write(&project, Format::Brief, "rodir"));

    std::fs::set_permissions(&rodir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(refusal.error.kind, OpErrorKind::PermissionDenied);
    assert_eq!(refusal.error.code, EXPORT_WRITE_FAILED);
    assert!(!rodir.join("brief.json").exists());
    assert!(!rodir.join("brief.json.tmp").exists());
}

#[test]
fn a_directory_that_is_a_file_is_invalid_input() {
    let project = project();
    project.write("spec/x.spec", "ORIGINAL");
    for name in ["spec/x.spec", "spec/x.spec/sub"] {
        let refusal = refused(write(&project, Format::Brief, name));
        assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput, "{name}");
        assert_eq!(refusal.error.code, NOT_A_DIRECTORY, "{name}");
        assert_eq!(refusal.subject, Some(ExportField::Directory), "{name}");
    }
    assert_eq!(
        std::fs::read_to_string(project.root().join("spec/x.spec")).unwrap(),
        "ORIGINAL"
    );
}

#[test]
fn a_view_with_no_root_is_refused_about_the_directory() {
    let project = project();
    let view = ProjectView::of(project.detached());
    let containment = Containment::unbounded();

    let refusal = refused(write_through(
        &view,
        &containment,
        request(Format::Brief),
        "out",
    ));

    assert_eq!(refusal.error.kind, OpErrorKind::InvalidInput);
    assert_eq!(refusal.error.code, DIRECTORY_WITHOUT_PROJECT);
    assert_eq!(refusal.subject, Some(ExportField::Directory));
    assert!(!project.root().join("out").exists());
}

#[test]
fn the_exports_own_refusals_keep_their_field() {
    let project = project();
    let containment = Containment::within(project.root());
    let refusal = refused(write_through(
        &project.view(),
        &containment,
        Request {
            scope: Some("ghost"),
            ..request(Format::Brief)
        },
        "out",
    ));
    assert_eq!(refusal.subject, Some(ExportField::Scope));
    assert_eq!(refusal.error.code, "E003");
    assert!(!project.root().join("out").exists());
}

#[test]
fn nothing_is_created_before_the_directory_is_held() {
    let project = project();
    assert_outside(&refused(write(&project, Format::Brief, "new/../../x")));
    assert!(!project.root().join("new").exists());
}

#[test]
fn the_written_export_carries_the_notices() {
    let project = project();
    let record = project.root().join(".specforge/schema-cache.json");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, r#"{"schema": {"#).unwrap();
    let containment = Containment::within(project.root());
    let caller = Caller::new(&containment, Asking::Nobody);
    let directory = Directory {
        name: Path::new("out"),
        caller,
    };

    let written = write_into(&project.view(), &request(Format::Brief), &directory).unwrap();

    let codes: Vec<&str> = written.notices().iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["I016"], "{:?}", written.notices());
}
