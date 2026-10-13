use crate::repo::Repo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The staged files; checks reaching `Hook` (none compiles the workspace).
    Hook,
    /// The working tree against HEAD; checks reaching `Hook` or `Quick`, by trigger.
    Quick,
    /// Every check reaching `Hook`, `Quick` or `Full`, triggers ignored.
    Full,
    /// `Full`, with nextest's `ci` profile, GitHub log groups and error annotations, and the pinned tools installed
    /// when missing.
    Ci,
    /// Checks reaching `Bootstrap`: a cold clone rebuilds every blob.
    Bootstrap,
}

/// What a check's `run` gets: the repo and the mode.
pub struct Ctx<'a> {
    pub repo: &'a Repo,
    pub mode: Mode,
}
