//! How set, unset, link, unlink and create read a `field.entry` path (ADR 0151, amended by round 16 plan 09).

use super::*;
use crate::navigate::compile;

const SOFTWARE_FORMAL: &[&str] = &["@specforge/software", "@specforge/formal"];

const PATHS: &str = "behavior check_graph \"Check the Graph\" {\n  category command\n  requires {\n    \
                     graph_ready \"Graph built\"\n    disk_ready  \"Disk readable\"\n  }\n  contract \"c\"\n}\n\n\
                     behavior plain \"Plain\" {\n  category command\n  contract \"c\"\n}\n\n\
                     type Task \"Task\" {\n  id string\n}\n";
const TEXT_BLOCK: &str =
    "behavior odd \"Odd\" {\n  category command\n  requires \"not a block\"\n  contract \"c\"\n}\n";

fn project_of(text: &str) -> Compiled {
    compile(SOFTWARE_FORMAL, &[("g.spec", text)])
}

#[derive(Clone, Copy)]
enum Want {
    Changes,
    Nothing,
    /// The refusal's code and a fragment of its message.
    Refused(&'static str, &'static str),
}
use Want::*;

const WHOLE: Want = Refused("not_editable", "takes a whole field");
const MALFORMED: Want = Refused("not_editable", "not a field or a field.entry path");
const TYPO: Want = Refused("unknown_field", "no field 'requries'");
const NOT_BLOCK: Want = Refused("not_editable", "'category' is not a block field");
const NOT_BLOCK_ID: Want = Refused("not_editable", "'id' is not a block field");

struct Row {
    id: &'static str,
    path: &'static str,
    set: Want,
    unset: Want,
    /// Asked of both link and unlink.
    link: Want,
    /// `None`: not asked.
    create: Option<Want>,
}

const ROWS: &[Row] = &[
    Row {
        id: "check_graph",
        path: "requires.graph_ready",
        set: Changes,
        unset: Changes,
        link: WHOLE,
        create: Some(WHOLE),
    },
    Row {
        id: "check_graph",
        path: "requires.net_ready",
        set: Changes,
        unset: Nothing,
        link: WHOLE,
        create: Some(WHOLE),
    },
    Row {
        id: "plain",
        path: "requires.net_ready",
        set: Changes,
        unset: Nothing,
        link: WHOLE,
        create: Some(WHOLE),
    },
    Row {
        id: "check_graph",
        path: "requries.graph_ready",
        set: TYPO,
        unset: TYPO,
        link: TYPO,
        create: Some(TYPO),
    },
    Row {
        id: "check_graph",
        path: "category.x",
        set: NOT_BLOCK,
        unset: NOT_BLOCK,
        link: NOT_BLOCK,
        create: Some(NOT_BLOCK),
    },
    Row {
        id: "check_graph",
        path: "requires.graph_ready.x",
        set: MALFORMED,
        unset: MALFORMED,
        link: MALFORMED,
        create: Some(MALFORMED),
    },
    Row {
        id: "check_graph",
        path: ".x",
        set: MALFORMED,
        unset: MALFORMED,
        link: MALFORMED,
        create: Some(MALFORMED),
    },
    Row {
        id: "check_graph",
        path: "requires.",
        set: MALFORMED,
        unset: MALFORMED,
        link: MALFORMED,
        create: Some(MALFORMED),
    },
    Row {
        id: "Task",
        path: "id.x",
        set: NOT_BLOCK_ID,
        unset: NOT_BLOCK_ID,
        link: NOT_BLOCK_ID,
        create: None,
    },
];

/// A line naming the mismatch, or `None` when `edit` came out as `want`.
fn check(c: &Compiled, label: &str, edit: Edit, want: Want) -> Option<String> {
    let got = match plan(c, &[edit]) {
        Ok(p) if p.is_empty() => Nothing,
        Ok(_) => Changes,
        Err(e) => {
            let e = e.error;
            let ok =
                matches!(want, Refused(code, frag) if e.code == code && e.message.contains(frag));
            if ok {
                if let Refused(..) = want
                    && label.contains("requries")
                {
                    assert_eq!(
                        e.suggestion.as_deref(),
                        Some("did you mean 'requires'?"),
                        "{label}"
                    );
                }
                return None;
            }
            return Some(format!("{label}: got Refused({}, {:?})", e.code, e.message));
        }
    };
    let same = matches!((want, got), (Changes, Changes) | (Nothing, Nothing));
    (!same).then(|| format!("{label}: wanted {}, got {}", name(want), name(got)))
}

fn name(w: Want) -> String {
    match w {
        Changes => "Changes".into(),
        Nothing => "Nothing".into(),
        Refused(code, frag) => format!("Refused({code}, {frag:?})"),
    }
}

fn verdict(mismatches: Vec<String>) {
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
#[ignore = "flipped by T2"]
fn set_and_unset_read_a_path_alike() {
    let c = project_of(PATHS);
    let mut bad = Vec::new();
    for r in ROWS {
        let label = |verb: &str| format!("{verb} {} {}", r.id, r.path);
        bad.extend(check(&c, &label("set"), set(r.id, r.path, one("t")), r.set));
        bad.extend(check(&c, &label("unset"), unset(r.id, r.path), r.unset));
    }
    verdict(bad);
}

#[test]
#[ignore = "flipped by T2"]
fn a_path_into_a_block_field_written_as_text_is_refused() {
    let c = project_of(TEXT_BLOCK);
    let want = Refused("not_editable", "written as a value, not a block");
    let mut bad = Vec::new();
    bad.extend(check(
        &c,
        "set odd requires.x",
        set("odd", "requires.x", one("hi")),
        want,
    ));
    bad.extend(check(
        &c,
        "unset odd requires.x",
        unset("odd", "requires.x"),
        want,
    ));
    verdict(bad);
}

#[test]
#[ignore = "flipped by T3"]
fn link_and_unlink_take_a_whole_field() {
    let c = project_of(PATHS);
    let mut bad = Vec::new();
    for r in ROWS {
        let label = |verb: &str| format!("{verb} {} {}", r.id, r.path);
        bad.extend(check(
            &c,
            &label("link"),
            link(r.id, r.path, "graph_ready"),
            r.link,
        ));
        bad.extend(check(
            &c,
            &label("unlink"),
            unlink(r.id, r.path, "graph_ready"),
            r.link,
        ));
    }
    let c = project_of(TEXT_BLOCK);
    bad.extend(check(
        &c,
        "link odd requires.x",
        link("odd", "requires.x", "a"),
        WHOLE,
    ));
    verdict(bad);
}

#[test]
#[ignore = "flipped by T3"]
fn create_takes_whole_fields() {
    let c = project_of(PATHS);
    let mut bad = Vec::new();
    for r in ROWS {
        let Some(want) = r.create else { continue };
        let edit = create("behavior", "fresh_one", "g.spec", &[(r.path, one("t"))]);
        bad.extend(check(&c, &format!("create {}", r.path), edit, want));
    }
    verdict(bad);
}
