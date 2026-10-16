//! Who can be asked to consent to what an operation would otherwise refuse (ADR 0110): a publisher key
//! change (add, update) or a collector's command (collect). The surface decides once per command and
//! hands the value to the operation; the operation, or the adapter it calls, puts each question to it.

use std::path::Path;

/// Where the key a signed package was expected to have comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedFrom {
    /// The user's pin in their known-keys store.
    UserPin,
    /// The key the project's `specforge.lock` records.
    Lock,
}

/// One question an operation puts to whoever can answer.
#[derive(Debug, Clone, Copy)]
pub enum Question<'q> {
    /// A signed package's publisher key is not the one expected for its name (add, update).
    KeyChange {
        /// The package's name.
        package: &'q str,
        /// The key expected for it.
        expected: &'q str,
        /// The key it is signed with.
        offered: &'q str,
        /// Where `expected` comes from.
        from: ExpectedFrom,
    },
    /// A collector's declared command would run in the project (collect).
    RunCommand {
        /// The extension that declares the collector.
        extension: &'q str,
        /// The command line.
        argv: &'q [String],
        /// The project root it runs in.
        root: &'q Path,
    },
}

/// Whoever answers: the CLI's terminal, a test's script. Called only for a question nothing else settled.
pub trait Asker {
    /// Yes or no.
    fn answer(&self, question: &Question<'_>) -> bool;
}

/// Who can be asked, given once to an operation.
#[derive(Clone, Copy)]
pub enum Asking<'a> {
    /// Every question is answered yes, and nobody is asked (`--yes`, for CI).
    Yes,
    /// Nobody can answer: every question is no (MCP, `--format json`, stdin that is no terminal).
    Nobody,
    /// Each question is put to the asker.
    Ask(&'a dyn Asker),
}

impl<'a> Asking<'a> {
    /// The one rule (ADR 0101 D1): `assume_yes` wins; else the asker, when someone can answer; else nobody.
    #[must_use]
    pub fn of(assume_yes: bool, asker: Option<&'a dyn Asker>) -> Self {
        match (assume_yes, asker) {
            (true, _) => Self::Yes,
            (false, Some(asker)) => Self::Ask(asker),
            (false, None) => Self::Nobody,
        }
    }

    /// The answer to `question`: yes, no, or the asker's.
    #[must_use]
    pub fn answers(&self, question: &Question<'_>) -> bool {
        match self {
            Self::Yes => true,
            Self::Nobody => false,
            Self::Ask(asker) => asker.answer(question),
        }
    }
}

impl std::fmt::Debug for Asking<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Yes => "Yes",
            Self::Nobody => "Nobody",
            Self::Ask(_) => "Ask",
        })
    }
}

/// A scripted asker for tests.
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    use super::{Asker, ExpectedFrom, Question};
    use std::cell::RefCell;

    /// An asker that gives one answer to every question and records each question asked.
    pub struct Scripted {
        answer: bool,
        asked: RefCell<Vec<Asked>>,
    }

    /// A question as [`Scripted`] recorded it (owned).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Asked {
        /// A publisher key change.
        KeyChange {
            /// The package's name.
            package: String,
            /// The key expected for it.
            expected: String,
            /// The key it is signed with.
            offered: String,
            /// Where `expected` comes from.
            from: ExpectedFrom,
        },
        /// A collector's command.
        RunCommand {
            /// The extension that declares the collector.
            extension: String,
            /// The command line.
            argv: Vec<String>,
        },
    }

    impl Scripted {
        /// An asker that answers `answer` to everything.
        #[must_use]
        pub fn answering(answer: bool) -> Self {
            Self {
                answer,
                asked: RefCell::new(Vec::new()),
            }
        }

        /// Every question put so far, in order.
        #[must_use]
        pub fn asked(&self) -> Vec<Asked> {
            self.asked.borrow().clone()
        }
    }

    impl Asker for Scripted {
        fn answer(&self, question: &Question<'_>) -> bool {
            self.asked.borrow_mut().push(match *question {
                Question::KeyChange {
                    package,
                    expected,
                    offered,
                    from,
                } => Asked::KeyChange {
                    package: package.to_string(),
                    expected: expected.to_string(),
                    offered: offered.to_string(),
                    from,
                },
                Question::RunCommand {
                    extension, argv, ..
                } => Asked::RunCommand {
                    extension: extension.to_string(),
                    argv: argv.to_vec(),
                },
            });
            self.answer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Asked, Scripted};
    use super::*;

    fn key_change() -> Question<'static> {
        Question::KeyChange {
            package: "@x/a",
            expected: "k1",
            offered: "k2",
            from: ExpectedFrom::UserPin,
        }
    }

    #[test]
    fn asking_of_is_yes_then_the_asker_then_nobody() {
        let asker = Scripted::answering(false);

        assert_eq!(format!("{:?}", Asking::of(true, Some(&asker))), "Yes");
        assert_eq!(format!("{:?}", Asking::of(true, None)), "Yes");
        assert_eq!(format!("{:?}", Asking::of(false, Some(&asker))), "Ask");
        assert_eq!(format!("{:?}", Asking::of(false, None)), "Nobody");
    }

    #[test]
    fn yes_and_nobody_answer_without_asking() {
        let held = Scripted::answering(true);

        assert!(Asking::Yes.answers(&key_change()));
        assert!(!Asking::Nobody.answers(&key_change()));
        assert!(held.asked().is_empty());
    }

    #[test]
    fn an_asker_is_put_each_question_and_its_answer_is_the_answer() {
        let scripted = Scripted::answering(false);
        let argv = vec!["cargo".to_string(), "test".to_string()];
        let asking = Asking::Ask(&scripted);

        assert!(!asking.answers(&key_change()));
        assert!(!asking.answers(&Question::RunCommand {
            extension: "@specforge/cargo-test",
            argv: &argv,
            root: Path::new("."),
        }));

        assert_eq!(
            scripted.asked(),
            vec![
                Asked::KeyChange {
                    package: "@x/a".to_string(),
                    expected: "k1".to_string(),
                    offered: "k2".to_string(),
                    from: ExpectedFrom::UserPin,
                },
                Asked::RunCommand {
                    extension: "@specforge/cargo-test".to_string(),
                    argv,
                },
            ]
        );
        assert!(Asking::Ask(&Scripted::answering(true)).answers(&key_change()));
    }
}
