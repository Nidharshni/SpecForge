//! `cargo gate`: the repository's check list. `cargo gate list` prints it.

use std::process::ExitCode;

const USAGE: &str = "usage: cargo gate list";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["list"] => {
            print!("{}", gate::render_list());
            ExitCode::SUCCESS
        }
        [
            "hook" | "quick" | "full" | "ci" | "bootstrap" | "baseline",
            ..,
        ] => {
            eprintln!("cargo gate {}: wired in the next ticket (T3)", args[0]);
            ExitCode::from(2)
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
