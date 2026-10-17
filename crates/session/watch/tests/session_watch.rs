//! The loop `specforge watch` runs, without a process: a session kept
//! current from a recorder standing where the file watchers do. The project
//! uses the real component runtime and a fixture extension whose `gadget`
//! kind names files (`docs`), so an edit can name a file the checks read.

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::json;
use specforge_project::{ProjectSession, UpdateKind, WatchRoot};
use specforge_test_macros::test as spec;
use specforge_watch::{Applied, SessionWatch, WatchEvent, Watchers};
use tempfile::TempDir;

type Hook = Rc<RefCell<Option<Box<dyn FnMut()>>>>;

/// Records what it is asked to watch; can run a closure when asked (a write
/// racing the move of the watchers) and refuse.
#[derive(Clone, Default)]
struct Recorder {
    armed: Rc<RefCell<Vec<Vec<WatchRoot>>>>,
    on_watch: Hook,
    refuse_after_start: Rc<RefCell<bool>>,
}

impl Recorder {
    fn on_watch(&self, hook: impl FnMut() + 'static) {
        *self.on_watch.borrow_mut() = Some(Box::new(hook));
    }

    fn refuse_from_now_on(&self) {
        *self.refuse_after_start.borrow_mut() = true;
    }

    fn armed(&self) -> Vec<Vec<WatchRoot>> {
        self.armed.borrow().clone()
    }
}

impl Watchers for Recorder {
    fn watch(&mut self, roots: &[WatchRoot]) -> Result<(), String> {
        let first = self.armed.borrow().is_empty();
        self.armed.borrow_mut().push(roots.to_vec());
        if let Some(hook) = self.on_watch.borrow_mut().as_mut() {
            hook();
        }
        if !first && *self.refuse_after_start.borrow() {
            return Err("boom".to_string());
        }
        Ok(())
    }
}

/// A project under `<parent>/proj` (spec root `spec/`, the docref fixture
/// extension, `spec/a.spec` holding `spec`) beside an empty `<parent>/outside`
/// directory and an empty `proj/docs`. Paths are canonical, as a watcher
/// reports them.
struct Fixture {
    _parent: TempDir,
    parent: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new(spec: &str) -> Self {
        let parent = TempDir::new().unwrap();
        let canonical = fs::canonicalize(parent.path()).unwrap();
        let root = canonical.join("proj");
        for dir in ["proj/spec", "proj/ext", "proj/docs", "outside"] {
            fs::create_dir_all(canonical.join(dir)).unwrap();
        }
        let config = json!({
            "name": "p",
            "version": "0.1.0",
            "spec_root": "spec",
            "extensions": ["@specforge/software", "@sdk/docref=ext/docref.wasm"],
        });
        fs::write(root.join("specforge.json"), config.to_string()).unwrap();
        fs::copy(
            specforge_component::committed::DOCREF.path(),
            root.join("ext/docref.wasm"),
        )
        .unwrap();
        fs::write(root.join("spec/a.spec"), spec).unwrap();
        Self {
            _parent: parent,
            parent: canonical,
            root,
        }
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn watch(&self) -> (SessionWatch<Recorder>, Recorder) {
        let recorder = Recorder::default();
        let watch = SessionWatch::start(
            ProjectSession::open(&specforge_common::ProjectRoot::of(&self.root).unwrap()),
            recorder.clone(),
        )
        .expect("the project is watched");
        (watch, recorder)
    }
}

const PLAIN: &str = "gadget gadget_one \"G\" {\n}\n";

/// A gadget naming `guide.md` in the sibling `outside/` directory (from the
/// spec root: `spec/../../outside/guide.md`).
const NAMES_OUTSIDE: &str = "gadget gadget_one \"G\" {\n  docs [\"../../outside/guide.md\"]\n}\n";

/// A gadget naming `docs/guide.md` under the project root.
const NAMES_INSIDE: &str = "gadget gadget_one \"G\" {\n  docs [\"../docs/guide.md\"]\n}\n";

fn codes(applied: &Applied) -> Vec<&str> {
    applied
        .update
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

fn applied(event: &WatchEvent) -> &Applied {
    match event {
        WatchEvent::Applied(applied) => applied,
        WatchEvent::Unwatched(e) => panic!("expected an update, the watchers failed: {e}"),
    }
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "an edit that names a file outside the watched directories moves the watchers"
)]
fn an_edit_naming_a_file_outside_the_roots_rearms_on_its_directory() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();
    assert_eq!(recorder.armed().len(), 1, "armed at start");

    fixture.write("spec/a.spec", NAMES_OUTSIDE);
    let events = watch.changed(&[fixture.root.join("spec/a.spec")]);

    assert_eq!(events.len(), 1, "{events:?}");
    let sources = applied(&events[0]);
    assert_eq!(sources.update.kind, UpdateKind::Sources);
    assert!(codes(sources).contains(&"E016"), "{:?}", codes(sources));
    let outside = WatchRoot {
        dir: fixture.parent.join("outside"),
        recursive: true,
    };
    let armed = recorder.armed();
    assert_eq!(armed.len(), 2, "re-armed once: {armed:?}");
    assert!(armed[1].contains(&outside), "{armed:?}");
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "after the watchers move, the session catches up on what changed while they did"
)]
fn what_changes_while_the_watchers_move_is_caught_up() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();
    let guide = fixture.parent.join("outside/guide.md");
    recorder.on_watch(move || fs::write(&guide, "# guide\n").unwrap());

    fixture.write("spec/a.spec", NAMES_OUTSIDE);
    let events = watch.changed(&[fixture.root.join("spec/a.spec")]);

    assert_eq!(events.len(), 2, "{events:?}");
    let first = applied(&events[0]);
    assert_eq!(first.update.kind, UpdateKind::Sources);
    assert!(codes(first).contains(&"E016"));
    let catch_up = applied(&events[1]);
    assert_eq!(catch_up.update.kind, UpdateKind::Checks);
    assert!(!codes(catch_up).contains(&"E016"), "{:?}", codes(catch_up));
    assert_eq!(catch_up.changed, catch_up.update.rebuilt_files);
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "an edit that names a file inside the watched directories moves nothing"
)]
fn an_edit_inside_the_roots_rearms_nothing() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();

    fixture.write("spec/a.spec", NAMES_INSIDE);
    let events = watch.changed(&[fixture.root.join("spec/a.spec")]);

    assert_eq!(events.len(), 1, "{events:?}");
    assert!(applied(&events[0]).update.inputs_changed);
    assert_eq!(recorder.armed().len(), 1, "{:?}", recorder.armed());
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "after spec_root changes, files under the new spec root are watched"
)]
fn a_moved_spec_root_rearms_and_catches_up() {
    let fixture = Fixture::new(PLAIN);
    // The new spec root is outside the project root, so no watched
    // directory covers it yet.
    let specs = fixture.parent.join("specs");
    fs::create_dir_all(&specs).unwrap();
    let (mut watch, recorder) = fixture.watch();
    // Its file is written while the watchers move.
    let three = specs.join("three.spec");
    recorder.on_watch(move || fs::write(&three, PLAIN.replace("gadget_one", "three")).unwrap());

    let config = json!({
        "name": "p",
        "version": "0.1.0",
        "spec_root": "../specs",
        "extensions": ["@specforge/software", "@sdk/docref=ext/docref.wasm"],
    });
    fixture.write("specforge.json", &config.to_string());
    let events = watch.changed(&[fixture.root.join("specforge.json")]);

    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(applied(&events[0]).update.kind, UpdateKind::Environment);
    let catch_up = applied(&events[1]);
    assert_eq!(catch_up.update.kind, UpdateKind::Sources);
    assert_eq!(catch_up.update.rebuilt_files, ["three.spec"]);
    let armed = recorder.armed();
    let moved = WatchRoot {
        dir: specs,
        recursive: true,
    };
    assert!(armed.last().unwrap().contains(&moved), "{armed:?}");
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "a failed move of the watchers is reported and the session still catches up"
)]
fn a_failed_rearm_is_reported_and_the_session_still_catches_up() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();
    recorder.refuse_from_now_on();
    let guide = fixture.parent.join("outside/guide.md");
    recorder.on_watch(move || fs::write(&guide, "# guide\n").unwrap());

    fixture.write("spec/a.spec", NAMES_OUTSIDE);
    let events = watch.changed(&[fixture.root.join("spec/a.spec")]);

    let [first, refused, catch_up] = &events[..] else {
        panic!("{events:?}");
    };
    assert_eq!(applied(first).update.kind, UpdateKind::Sources);
    assert!(
        matches!(refused, WatchEvent::Unwatched(e) if e == "boom"),
        "{refused:?}"
    );
    assert_eq!(applied(catch_up).update.kind, UpdateKind::Checks);
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "a failed move of the watchers is reported and the session still catches up"
)]
fn a_failed_rearm_with_nothing_stale_ends_on_the_report() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();
    recorder.refuse_from_now_on();

    fixture.write("spec/a.spec", NAMES_OUTSIDE);
    let events = watch.changed(&[fixture.root.join("spec/a.spec")]);

    let [first, refused] = &events[..] else {
        panic!("{events:?}");
    };
    assert_eq!(applied(first).update.kind, UpdateKind::Sources);
    assert!(
        matches!(refused, WatchEvent::Unwatched(e) if e == "boom"),
        "{refused:?}"
    );
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "what was written between the open and the watchers is applied before ready"
)]
fn what_was_written_before_the_watchers_is_applied_at_the_start() {
    let fixture = Fixture::new(PLAIN);
    let session = ProjectSession::open(&specforge_common::ProjectRoot::of(&fixture.root).unwrap());
    fixture.write("spec/b.spec", &PLAIN.replace("gadget_one", "gadget_two"));
    let mut watch = SessionWatch::start(session, Recorder::default()).unwrap();

    let events = watch.catch_up();

    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(applied(&events[0]).update.rebuilt_files, ["b.spec"]);
    assert!(watch.catch_up().is_empty(), "nothing is stale now");
}

#[test]
fn changed_paths_are_named_by_key_or_by_path_under_the_root() {
    let fixture = Fixture::new(NAMES_OUTSIDE);
    let (mut watch, _) = fixture.watch();
    let guide = fixture.parent.join("outside/guide.md");
    fs::write(&guide, "# guide\n").unwrap();
    // A path changes something only when its file is not as the session
    // read it: write the other two again, differently.
    fixture.write("spec/a.spec", &format!("{NAMES_OUTSIDE}\n"));
    let config = fs::read_to_string(fixture.root.join("specforge.json")).unwrap();
    fixture.write("specforge.json", &format!("{config}\n"));

    let events = watch.changed(&[
        fixture.root.join("spec/a.spec"),
        fixture.root.join("specforge.json"),
        guide.clone(),
    ]);

    // A reload covers all three; the batch names them: the outside path
    // absolute, the config under the root, the source by its key.
    let named = &applied(&events[0]).changed;
    assert_eq!(
        named,
        &[
            guide.to_string_lossy().into_owned(),
            "a.spec".to_string(),
            "specforge.json".to_string()
        ]
    );
}

#[test]
fn a_batch_that_changes_nothing_reports_nothing() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();
    fixture.write("notes.txt", "not an input");

    assert!(watch.changed(&[fixture.root.join("notes.txt")]).is_empty());
    assert!(watch.changed(&[]).is_empty());
    assert_eq!(recorder.armed().len(), 1);
}

#[test]
fn a_source_rewritten_with_its_own_text_reports_nothing() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, recorder) = fixture.watch();
    fixture.write("spec/a.spec", PLAIN);
    let a = fixture.root.join("spec/a.spec");
    let file = fs::OpenOptions::new().write(true).open(&a).unwrap();
    let modified = fs::metadata(&a).unwrap().modified().unwrap();
    file.set_modified(modified + std::time::Duration::from_secs(3))
        .unwrap();

    assert!(watch.changed(&[a]).is_empty());
    assert_eq!(recorder.armed().len(), 1);
}

#[test]
fn start_refuses_a_session_built_from_no_directory() {
    let Err(e) = SessionWatch::start(ProjectSession::detached(), Recorder::default()) else {
        panic!("a detached session is built from no directory");
    };
    assert!(e.starts_with("failed to watch directory: "), "{e}");
    assert!(e.ends_with(" does not exist"), "{e}");

    // A project root that was removed after it was opened.
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("gone");
    std::fs::create_dir(&missing).unwrap();
    let session = ProjectSession::open(&specforge_common::ProjectRoot::of(&missing).unwrap());
    std::fs::remove_dir(&missing).unwrap();
    let Err(e) = SessionWatch::start(session, Recorder::default()) else {
        panic!("a missing project root is watched");
    };
    assert_eq!(
        e,
        format!(
            "failed to watch directory: {} does not exist",
            missing.display()
        )
    );
}

#[test]
fn applied_says_what_the_update_left() {
    let fixture = Fixture::new(PLAIN);
    let (mut watch, _) = fixture.watch();
    fixture.write("spec/b.spec", &PLAIN.replace("gadget_one", "gadget_two"));

    let events = watch.changed(&[fixture.root.join("spec/b.spec")]);

    let left = applied(&events[0]);
    assert_eq!(left.files, watch.session().project().file_count());
    assert_eq!(left.files, 2);
    assert_eq!(left.nodes, watch.session().project().graph().node_count());
    assert!(
        left.extensions.iter().any(|name| name == "@sdk/docref"),
        "{:?}",
        left.extensions
    );
    assert_eq!(left.changed, ["b.spec"]);
}

#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "a directory event reaches every source under it"
)]
fn a_directory_event_reaches_every_source_under_it() {
    let fixture = Fixture::new(PLAIN);
    fixture.write(
        "spec/old/b.spec",
        &PLAIN.replace("gadget_one", "gadget_two"),
    );
    let (mut watch, _recorder) = fixture.watch();

    fs::rename(fixture.root.join("spec/old"), fixture.root.join("spec/new")).unwrap();
    let events = watch.changed(&[fixture.root.join("spec/new"), fixture.root.join("spec/old")]);

    assert_eq!(events.len(), 1, "{events:?}");
    let moved = applied(&events[0]);
    assert_eq!(moved.changed, ["new/b.spec", "old/b.spec"]);
    assert_eq!(moved.update.rebuilt_files, ["new/b.spec", "old/b.spec"]);
    assert!(watch.session().stale().is_empty());
}

#[cfg(unix)]
#[spec(
    behavior = "classify_project_changes",
    verify = "a session's watch roots and the LSP's watchers cover every symlinked source's target"
)]
fn a_symlinked_sources_target_is_watched() {
    let fixture = Fixture::new(PLAIN);
    let outside = fixture.parent.join("outside");
    let target = outside.join("real.spec");
    fs::write(&target, PLAIN.replace("gadget_one", "gadget_three")).unwrap();
    std::os::unix::fs::symlink(&target, fixture.root.join("spec/link.spec")).unwrap();
    let (mut watch, recorder) = fixture.watch();

    assert!(
        recorder.armed()[0].contains(&WatchRoot {
            dir: outside,
            recursive: true
        }),
        "{:?}",
        recorder.armed()
    );
    fs::write(
        &target,
        format!("{}\n", PLAIN.replace("gadget_one", "gadget_three")),
    )
    .unwrap();
    let events = watch.changed(&[target]);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(applied(&events[0]).changed, ["link.spec"]);
}

#[cfg(unix)]
#[spec(
    behavior = "classify_project_changes",
    verify = "a symlinked source is reached by each hop its read followed"
)]
fn a_sources_middle_hop_pointed_elsewhere_is_seen() {
    let fixture = Fixture::new(PLAIN);
    let real = fixture.parent.join("real");
    fs::create_dir_all(&real).unwrap();
    fs::write(
        real.join("real.spec"),
        PLAIN.replace("gadget_one", "gadget_three"),
    )
    .unwrap();
    let hop = fixture.parent.join("outside/l1");
    std::os::unix::fs::symlink(real.join("real.spec"), &hop).unwrap();
    std::os::unix::fs::symlink(&hop, fixture.root.join("spec/link.spec")).unwrap();
    let (mut watch, recorder) = fixture.watch();

    assert!(
        recorder.armed()[0].contains(&WatchRoot {
            dir: fixture.parent.join("outside"),
            recursive: true
        }),
        "{:?}",
        recorder.armed()
    );
    fs::write(
        real.join("other.spec"),
        format!("{}\n", PLAIN.replace("gadget_one", "gadget_four")),
    )
    .unwrap();
    fs::remove_file(&hop).unwrap();
    std::os::unix::fs::symlink(real.join("other.spec"), &hop).unwrap();
    let events = watch.changed(&[hop]);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(applied(&events[0]).changed, ["link.spec"]);
}

#[cfg(unix)]
#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "a symlinked source whose hop lies in a directory that did not exist is seen when it is created"
)]
fn a_hop_into_a_missing_directory_is_followed_when_it_is_created() {
    let fixture = Fixture::new(PLAIN);
    let real = fixture.parent.join("real");
    fs::create_dir_all(&real).unwrap();
    fs::write(
        real.join("real.spec"),
        PLAIN.replace("gadget_one", "gadget_three"),
    )
    .unwrap();
    let hop = fixture.parent.join("outside/l1");
    std::os::unix::fs::symlink(real.join("real.spec"), &hop).unwrap();
    std::os::unix::fs::symlink(&hop, fixture.root.join("spec/link.spec")).unwrap();
    let far = fixture.parent.join("far");
    fs::create_dir_all(&far).unwrap();
    let (mut watch, recorder) = fixture.watch();

    fs::remove_file(&hop).unwrap();
    std::os::unix::fs::symlink(far.join("newdir/final.spec"), &hop).unwrap();
    let events = watch.changed(&[hop]);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(applied(&events[0]).changed, ["link.spec"]);
    assert!(
        recorder.armed().last().unwrap().contains(&WatchRoot {
            dir: far.clone(),
            recursive: false
        }),
        "{:?}",
        recorder.armed()
    );

    fs::create_dir(far.join("newdir")).unwrap();
    fs::write(
        far.join("newdir/final.spec"),
        PLAIN.replace("gadget_one", "gadget_five"),
    )
    .unwrap();
    let events = watch.changed(&[far.join("newdir")]);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(applied(&events[0]).changed, ["link.spec"]);
    assert!(
        recorder.armed().last().unwrap().contains(&WatchRoot {
            dir: far.join("newdir"),
            recursive: true
        }),
        "{:?}",
        recorder.armed()
    );
}

#[cfg(unix)]
#[spec(
    behavior = "watch_file_system_for_changes",
    verify = "a symlinked source whose hop lies in a directory that did not exist is seen when it is created"
)]
#[ignore = "flipped by T4"]
fn a_dangling_source_links_missing_directory_outside_the_root_is_watched() {
    let fixture = Fixture::new(PLAIN);
    let far = fixture.parent.join("far");
    fs::create_dir_all(&far).unwrap();
    std::os::unix::fs::symlink(
        far.join("newdir/final.spec"),
        fixture.root.join("spec/link.spec"),
    )
    .unwrap();
    let (mut watch, recorder) = fixture.watch();

    assert!(
        recorder.armed()[0].contains(&WatchRoot {
            dir: far.clone(),
            recursive: false
        }),
        "{:?}",
        recorder.armed()
    );
    fs::create_dir(far.join("newdir")).unwrap();
    fs::write(
        far.join("newdir/final.spec"),
        PLAIN.replace("gadget_one", "gadget_five"),
    )
    .unwrap();
    let events = watch.changed(&[far.join("newdir")]);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(applied(&events[0]).changed, ["link.spec"]);
}
