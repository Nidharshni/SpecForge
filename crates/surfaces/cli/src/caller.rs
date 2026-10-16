//! What the CLI lends every operation it calls (ADR 0124): every path, since its user is already at the file
//! system (ADR 0092 D6), and its terminal to ask when someone can answer (ADR 0110 D5).

use crate::outcome::OutputFormat;
use crate::terminal::Terminal;
use specforge_ops::{Caller, Containment};

/// Every path: the CLI's reach.
static UNBOUNDED: Containment = Containment::unbounded();

/// The caller of one command: `--yes` answers every question; else the terminal, under human output with someone
/// at stdin; else nobody. A command with no `--yes` passes `false`: it has nothing to consent to with it.
pub(crate) fn of<'t>(yes: bool, format: OutputFormat, terminal: &'t Terminal) -> Caller<'t> {
    Caller::new(&UNBOUNDED, crate::asking::of(yes, format, terminal))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;
    use crate::terminal::Facts;

    const INTERACTIVE: Facts = Facts {
        color_out: false,
        color_err: false,
        interactive: true,
    };

    #[test]
    fn the_cli_lends_every_path_and_asks_as_asking_says() {
        for (yes, format, facts) in [
            (true, OutputFormat::Human, Facts::PIPED),
            (true, OutputFormat::Json, INTERACTIVE),
            (false, OutputFormat::Json, INTERACTIVE),
            (false, OutputFormat::Human, Facts::PIPED),
            (false, OutputFormat::Human, INTERACTIVE),
            (true, OutputFormat::Human, INTERACTIVE),
        ] {
            let (terminal, _) = Terminal::captured("", facts);
            let caller = of(yes, format, &terminal);
            assert_eq!(caller.containment().root(), None);
            assert_eq!(
                format!("{:?}", caller.asking()),
                format!("{:?}", crate::asking::of(yes, format, &terminal))
            );
        }
    }

    /// Every `.rs` file under `dir` as `(name, text up to its test module)`.
    fn sources(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, base, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let name = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).unwrap();
                let lines: Vec<&str> = text.lines().collect();
                let end = (0..lines.len())
                    .find(|&i| {
                        lines[i].starts_with("#[cfg(test)]")
                            && lines.get(i + 1).is_some_and(|next| next.contains("mod "))
                    })
                    .unwrap_or(lines.len());
                out.push((name, lines[..end].join("\n")));
            }
        }
    }

    /// A command builds no reach and no asker of its own: `caller::of` is the one place (ADR 0124).
    #[test]
    fn only_the_caller_names_a_reach_or_an_asker() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &src, &mut files);
        let mut problems = Vec::new();
        for (name, text) in &files {
            if name == "caller.rs" || name == "asking.rs" {
                continue;
            }
            for (number, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for word in ["Containment::", "Asking::", "asking::of("] {
                    if line.contains(word) {
                        problems.push(format!(
                            "src/{name}:{}: names `{word}` beside caller::of",
                            number + 1
                        ));
                    }
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}
