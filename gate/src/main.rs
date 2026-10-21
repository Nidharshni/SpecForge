//! `cargo gate`: the repository's check list.
//!
//! ```text
//! cargo gate                 quick (default): what the working tree changed
//! cargo gate hook            what .githooks/pre-commit runs, on the staged files
//! cargo gate full            every check but the bootstrap's
//! cargo gate ci              what CI's main job runs
//! cargo gate bootstrap       what CI's cold job runs
//! cargo gate baseline        lower gate/baseline.json to what the checks find today (never raises an entry)
//! cargo gate baseline --only specs|file-size [--since <rev>]
//!                            one section; a file git saw move since <rev> (default HEAD) carries its entry
//! cargo gate crate-map       rewrite the crate map in ARCHITECTURE.md and crates/README.md
//! cargo gate list            print the table: name, reach, trigger, proves
//! cargo gate <mode> --only <check>[,<check>...]
//! ```
//!
//! Exit codes: 0 every selected check passed, 1 one failed or was blocked, 2 usage.
#![expect(
    clippy::print_stdout,
    reason = "cargo gate prints the check list and its run on stdout"
)]

use std::process::ExitCode;

use gate::{Mode, Repo};

const USAGE: &str = "usage: cargo gate [hook|quick|full|ci|bootstrap] [--only <check>[,<check>...]] | baseline [--only specs|file-size] [--since <rev>] | crate-map | list";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut mode, mut only, mut since, mut word) = (Mode::Quick, None, None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--only" => match rest.next() {
                Some(name) => only = Some(name.as_str()),
                None => return usage("--only needs a check name"),
            },
            "--since" => match rest.next() {
                Some(rev) => since = Some(rev.as_str()),
                None => return usage("--since needs a revision"),
            },
            "list" | "baseline" | "crate-map" if word.is_none() => word = Some(arg.as_str()),
            name => match Mode::parse(name) {
                Some(parsed) if word.is_none() => {
                    mode = parsed;
                    word = Some(name);
                }
                _ => return usage(&format!("unexpected `{name}`")),
            },
        }
    }
    if since.is_some() && word != Some("baseline") {
        return usage("--since is for `baseline`");
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
        return match gate::baseline(&repo, only, since) {
            Ok(lines) => {
                lines.iter().for_each(|l| println!("{l}"));
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        };
    }
    if word == Some("crate-map") {
        return match gate::crate_map::write(&repo) {
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
