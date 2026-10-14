//! `cargo gate`: the repository's check list.
//!
//! ```text
//! cargo gate                 quick (default): what the working tree changed
//! cargo gate hook            what .githooks/pre-commit runs, on the staged files
//! cargo gate full            every check but the bootstrap's
//! cargo gate ci              what CI's main job runs
//! cargo gate bootstrap       what CI's cold job runs
//! cargo gate baseline        lower gate/baseline.json to what the checks find today (never raises an entry)
//! cargo gate baseline --only specs
//!                            one section
//! cargo gate list            print the table: name, reach, trigger, proves
//! cargo gate <mode> --only <check>
//! ```
//!
//! Exit codes: 0 every selected check passed, 1 one failed or was blocked, 2 usage.

use std::process::ExitCode;

use gate::{Mode, Repo};

const USAGE: &str = "usage: cargo gate [hook|quick|full|ci|bootstrap] [--only <check>] | baseline [--only specs] | list";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut mode, mut only, mut word) = (Mode::Quick, None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--only" => match rest.next() {
                Some(name) => only = Some(name.as_str()),
                None => return usage("--only needs a check name"),
            },
            "list" | "baseline" if word.is_none() => word = Some(arg.as_str()),
            name => match Mode::parse(name) {
                Some(parsed) if word.is_none() => {
                    mode = parsed;
                    word = Some(name);
                }
                _ => return usage(&format!("unexpected `{name}`")),
            },
        }
    }
    if word == Some("list") {
        print!("{}", gate::render_list());
        return ExitCode::SUCCESS;
    }
    let repo = match Repo::discover() {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    if word == Some("baseline") {
        return match gate::baseline(&repo, only) {
            Ok(lines) => {
                lines.iter().for_each(|l| println!("{l}"));
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        };
    }
    let report = match gate::run(mode, &repo, only) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    let mut stdout = std::io::stdout();
    if report.print(mode, &mut stdout).is_err() {
        return ExitCode::from(1);
    }
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn usage(why: &str) -> ExitCode {
    eprintln!("cargo gate: {why}\n{USAGE}");
    ExitCode::from(2)
}

fn fail(why: &str) -> ExitCode {
    eprintln!("cargo gate: {why}");
    ExitCode::from(1)
}
