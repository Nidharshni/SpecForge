//! A project opened in a session's place (ADR 0109).

use specforge_project::testing::{TestProject, disk_changed};
use specforge_test::prelude::*;

const ALPHA: &str = "type alpha \"A\" {}\n";

#[specforge_test(
    behavior = "validate_delta_correctness",
    verify = "an environment reload keeps whether the session verifies its updates"
)]
fn a_reload_keeps_whether_the_session_verifies() {
    let dir = TestProject::new().file("a.spec", ALPHA);
    let mut session = dir.session_bare();

    session.set_verify_incremental(false);
    session.reload_environment();
    dir.write("a.spec", "type alpha \"B\" {}\n");
    let update = disk_changed(&mut session, &["a.spec"]).unwrap();
    assert_eq!(update.verification, None, "verification stayed off");

    session.set_verify_incremental(true);
    session.reload_environment();
    dir.write("a.spec", "type alpha \"C\" {}\n");
    let update = disk_changed(&mut session, &["a.spec"]).unwrap();
    assert_eq!(update.verification, Some(Ok(())), "verification stayed on");
}
