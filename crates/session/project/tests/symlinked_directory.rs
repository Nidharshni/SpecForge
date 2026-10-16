//! An editor buffer of a file reached through a symlinked directory under
//! the spec root is no source: discovery never enters that directory
//! (ADR 0073 D5).

use specforge_project::Buffer;
use specforge_project::testing::TestProject;

const ALPHA: &str = "behavior alpha \"A\" {\n}\n";

#[cfg(unix)]
#[test]
#[ignore = "flipped by T2"]
fn a_buffer_under_a_symlinked_directory_builds_nothing() {
    let dir = TestProject::new().files(&[("sub/a.spec", ALPHA)]);
    let root = dir.root().to_path_buf();
    std::os::unix::fs::symlink(root.join("sub"), root.join("dirlink")).unwrap();
    let mut session = dir.session_bare();
    assert_eq!(session.project().file_count(), 1);

    let update = session.hold(&[Buffer::new(root.join("dirlink/a.spec"), ALPHA)]);
    assert!(
        update.rebuilt_files.is_empty(),
        "{:?}",
        update.rebuilt_files
    );
    assert_eq!(session.project().file_count(), 1);
    assert!(
        session
            .project()
            .diagnostics()
            .iter()
            .all(|d| d.code != "E002")
    );

    session.release(&[root.join("dirlink/a.spec")]);
    assert_eq!(session.project().file_count(), 1);
    assert!(session.stale().is_empty());
}
