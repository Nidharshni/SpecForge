//! One change to project files, written all or nothing (ADR 0128): each file is staged with what it holds and what it
//! will hold, checked, replaced in stage order, and put back in reverse when anything fails. What the change left is
//! its failure's writes, so an operation's error can never drop it (ADR 0022 D4).
//!
//! The four steps of [`ProjectChange::commit_then`]:
//! 1. **Check.** Each staged file is read through its [`Held`], in stage order, and compared with the bytes it was
//!    staged over. Any difference is [`Cause::Changed`]; a read that fails is [`Cause::Unreadable`]. Nothing has
//!    been written.
//! 2. **Write**, in stage order. The missing directories are made (and remembered). A file that was there is
//!    [`Held::replace`]d, so it holds the new content or the old, never part of either; one that was not is
//!    [`Held::create_new`]d, so a file that appeared since the check is `Changed`.
//! 3. **Then.** The caller's last step runs once every file is written.
//! 4. **Put back**, on any failure, in reverse stage order: a file that was there gets its bytes back, one that was
//!    not is unlinked where it landed, and the directories the change made are removed when empty. A file that
//!    cannot be put back is named in `left`, which is the failure's writes.

use std::convert::Infallible;
use std::path::PathBuf;

use crate::{Held, OpError, Writes};

/// Project files staged to be written together.
pub(crate) struct ProjectChange {
    staged: Vec<Staged>,
}

struct Staged {
    held: Held,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

/// Why a change failed, and what it could not put back.
#[derive(Debug)]
pub(crate) struct Failed<E> {
    pub(crate) cause: Cause<E>,
    pub(crate) left: Writes,
}

/// What stopped a change. A file is named as its caller spelled it.
#[derive(Debug)]
pub(crate) enum Cause<E> {
    /// `file` no longer holds what it was staged over (or appeared, or went): nothing, or no file but those put
    /// back, was written.
    Changed { file: PathBuf },
    /// `file` could not be read to check it: nothing was written.
    Unreadable {
        file: PathBuf,
        error: std::io::Error,
    },
    /// `file` could not be written; it is as it was, and every file before it was put back but `left`.
    Unwritable {
        file: PathBuf,
        error: std::io::Error,
    },
    /// The caller's last step failed; every file was put back but `left`.
    Then(E),
}

impl ProjectChange {
    pub(crate) fn new() -> Self {
        ProjectChange { staged: Vec::new() }
    }

    /// Stage `held` to hold `after`, over `before`: the bytes the caller planned on, or `None` for a file that is
    /// not there. A file whose `after` is its `before` is not staged. Each `real` is staged at most once.
    pub(crate) fn stage(&mut self, held: Held, before: Option<Vec<u8>>, after: Vec<u8>) {
        debug_assert!(
            self.staged.iter().all(|s| s.held.real != held.real),
            "{} is staged twice",
            held.spelled.display()
        );
        if before.as_deref() == Some(after.as_slice()) {
            return;
        }
        self.staged.push(Staged {
            held,
            before,
            after,
        });
    }

    /// [`Self::commit_then`] with no last step.
    pub(crate) fn commit(self) -> Result<Writes, Failed<Infallible>> {
        self.commit_then(|| Ok(()))
    }

    /// Check, write, then run `then`; see the module docs for the order and what a failure puts back. On success
    /// the writes are every staged file, as spelled.
    pub(crate) fn commit_then<E>(
        self,
        then: impl FnOnce() -> Result<(), E>,
    ) -> Result<Writes, Failed<E>> {
        for staged in &self.staged {
            match staged.held.read_if_there() {
                Ok(now) if now == staged.before => {}
                Ok(_) => {
                    return Err(Self::refused(Cause::Changed {
                        file: staged.held.spelled.clone(),
                    }));
                }
                Err(error) => {
                    return Err(Self::refused(Cause::Unreadable {
                        file: staged.held.spelled.clone(),
                        error,
                    }));
                }
            }
        }

        let mut written = 0;
        let mut made = Vec::new();
        for staged in &self.staged {
            if let Err(cause) = Self::write(staged, &mut made) {
                return Err(self.put_back(written, &made, cause));
            }
            written += 1;
        }
        match then() {
            Ok(()) => Ok(self
                .staged
                .into_iter()
                .map(|staged| staged.held.spelled)
                .collect()),
            Err(e) => Err(self.put_back(written, &made, Cause::Then(e))),
        }
    }

    fn refused<E>(cause: Cause<E>) -> Failed<E> {
        Failed {
            cause,
            left: Writes::none(),
        }
    }

    /// One staged file written; `made` collects the directories created for it.
    fn write<E>(staged: &Staged, made: &mut Vec<PathBuf>) -> Result<(), Cause<E>> {
        let held = &staged.held;
        let file = || held.spelled.clone();
        held.create_parents(made)
            .map_err(|error| Cause::Unwritable {
                file: file(),
                error,
            })?;
        match &staged.before {
            Some(_) => held
                .replace(&staged.after)
                .map_err(|error| Cause::Unwritable {
                    file: file(),
                    error,
                }),
            None => match held.create_new(&staged.after) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    Err(Cause::Changed { file: file() })
                }
                Err(error) => {
                    // The open may have made the file before the write failed.
                    let _ = held.remove_created();
                    Err(Cause::Unwritable {
                        file: file(),
                        error,
                    })
                }
            },
        }
    }

    /// Put back the first `written` files in reverse, then remove `made` when empty.
    fn put_back<E>(self, written: usize, made: &[PathBuf], cause: Cause<E>) -> Failed<E> {
        let mut left = Writes::none();
        for staged in self.staged[..written].iter().rev() {
            let held = &staged.held;
            let restored = match &staged.before {
                Some(before) => match held.read_if_there() {
                    Ok(Some(now)) if now == *before => Ok(()),
                    _ => held.replace(before),
                },
                None => match held.remove_created() {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                },
            };
            if restored.is_err() {
                left.record(held.spelled.clone());
            }
        }
        let deepest_first: Vec<PathBuf> = made.iter().rev().cloned().collect();
        Held::remove_empty(&deepest_first);
        Failed { cause, left }
    }
}

impl<E> Failed<E> {
    /// The operation's error for this failure: `describe` words the cause, and the files left are its writes.
    pub(crate) fn error(self, describe: impl FnOnce(Cause<E>) -> OpError) -> OpError {
        describe(self.cause).with_writes(self.left)
    }
}

#[cfg(test)]
mod tests;
