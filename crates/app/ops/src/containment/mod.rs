//! What a request may reach on disk (ADR 0104): every path, or the paths that land inside one
//! directory. An operation applies it where it opens or writes the file, so the name it checks is
//! the name it opens. The CLI is unbounded (ADR 0092 D6); MCP bounds a request (ADR 0092 D2).
//!
//! A name is resolved the way the file system would: a relative one is read against the root
//! (bounded) or the process's directory (unbounded), `.` and `..` are applied to the real directory
//! reached so far, and every symlink on the way is followed, a dangling one by its target text.
//! What does not exist yet (a file about to be created) is the rest of the name, joined on.

mod disk;
mod file;

pub use disk::ContainedDisk;
pub use file::{ReadFailure, declared};

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::{OpError, OpErrorKind};

/// The code of every refusal [`Outside`] becomes through `From` (an `OpError` code string, not a
/// catalogue diagnostic: ADR 0092 D5).
pub const OUTSIDE_ROOT: &str = "outside_root";

/// The most symlinks one name may go through before it lands at itself (the open then fails).
const MAX_LINKS: usize = 40;

/// What a request may reach. Built by the surface, carried in the operation's request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Containment {
    /// The directory, canonical as far as it existed when built; `None`: unbounded.
    root: Option<PathBuf>,
}

/// A path a containment holds. The caller opens (reads, rewrites, creates) `real` and names
/// `spelled` in its messages, documents and records: never the other way round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// As the caller gave it, joined onto the root when bounded and relative.
    pub spelled: PathBuf,
    /// Where it lands: canonical as far as it exists, a missing tail joined on.
    pub real: PathBuf,
}

/// A name that lands outside its containment's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outside {
    named: PathBuf,
    landed: PathBuf,
    root: PathBuf,
}

impl Containment {
    /// Every path: [`resolve`](Self::resolve) hands a name back as given and refuses nothing.
    pub const fn unbounded() -> Self {
        Containment { root: None }
    }

    /// The paths that land inside `root`, as it is on disk now.
    pub fn within(root: &Path) -> Self {
        Containment {
            root: Some(landing(root)),
        }
    }

    /// This containment as the `Disk` port: what a session reads its sources through when the surface
    /// bounds them (ADR 0145).
    pub fn disk(&self) -> ContainedDisk {
        ContainedDisk(self.clone())
    }

    /// The same kind of bound for a project inside this one: `within(dir)` when bounded,
    /// unbounded when unbounded. A change bounds its writes to its own project (ADR 0092 D8).
    pub fn narrowed(&self, dir: &Path) -> Self {
        match self.root {
            Some(_) => Containment::within(dir),
            None => Containment::unbounded(),
        }
    }

    /// The directory, canonical; `None` when unbounded.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Where `named` really is, and how to name it. Bounded: `named` is read against the root (an
    /// absolute name stands); `Outside` unless it lands inside. Unbounded: a relative name is the
    /// process's, as the CLI's user typed it, and nothing is refused. Either way the real path is
    /// computed component by component, so what the caller opens is what was checked.
    pub fn resolve(&self, named: &Path) -> Result<Held, Outside> {
        let Some(root) = &self.root else {
            return Ok(Held {
                spelled: named.to_path_buf(),
                real: landing(named),
            });
        };
        let spelled = root.join(named);
        let real = landing(&spelled);
        if real.starts_with(root) {
            Ok(Held { spelled, real })
        } else {
            Err(Outside {
                named: named.to_path_buf(),
                landed: real,
                root: root.clone(),
            })
        }
    }
}

impl Outside {
    /// The refusal for `named` (a spelling of the caller's), which landed at `landed`, outside
    /// `root`.
    pub fn new(named: &Path, landed: PathBuf, root: &Path) -> Self {
        Outside {
            named: named.to_path_buf(),
            landed,
            root: root.to_path_buf(),
        }
    }

    /// As the caller spelled it.
    pub fn named(&self) -> &Path {
        &self.named
    }

    /// Where it really is.
    pub fn landed(&self) -> &Path {
        &self.landed
    }

    /// What bounds it.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// `invalid_input`, [`OUTSIDE_ROOT`]: "'<named>' resolves to <landed>, outside <root>, which bounds
/// the request". An operation that keeps its own refusal (migrate, infer, format's per-file
/// failure) builds it from the accessors instead.
impl From<Outside> for OpError {
    fn from(outside: Outside) -> Self {
        OpError::new(
            OpErrorKind::InvalidInput,
            OUTSIDE_ROOT,
            format!(
                "'{}' resolves to {}, outside {}, which bounds the request",
                outside.named.display(),
                outside.landed.display(),
                outside.root.display()
            ),
        )
    }
}

/// One step of a path still to walk.
enum Step {
    Root(OsString),
    Parent,
    Name(OsString),
}

fn steps(path: &Path) -> impl Iterator<Item = Step> {
    path.components().filter_map(|component| match component {
        Component::Prefix(_) | Component::RootDir => Some(Step::Root(component.as_os_str().into())),
        Component::CurDir => None,
        Component::ParentDir => Some(Step::Parent),
        Component::Normal(name) => Some(Step::Name(name.to_os_string())),
    })
}

/// Where `path` lands: made absolute and walked component by component. `..` leaves the real
/// directory reached so far (after its symlinks); every existing component is replaced by its
/// canonical form; a symlink whose target does not exist is replaced by its target (read against
/// the link's directory) and the walk goes on, at most [`MAX_LINKS`] times (then the name lands at
/// itself and the open fails). A missing tail is joined on.
fn landing(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut pending: VecDeque<Step> = steps(&path).collect();
    let mut landed = PathBuf::new();
    let mut links = 0;
    while let Some(step) = pending.pop_front() {
        match step {
            Step::Root(root) => landed.push(root),
            Step::Parent => {
                landed.pop();
            }
            Step::Name(name) => {
                landed.push(&name);
                if let Ok(real) = std::fs::canonicalize(&landed) {
                    landed = real;
                } else if links < MAX_LINKS
                    && let Ok(target) = std::fs::read_link(&landed)
                {
                    links += 1;
                    landed.pop();
                    for step in steps(&target).collect::<Vec<_>>().into_iter().rev() {
                        pending.push_front(step);
                    }
                    if target.is_absolute() {
                        landed = PathBuf::new();
                    }
                }
            }
        }
    }
    landed
}

#[cfg(test)]
mod disk_tests;
#[cfg(test)]
mod replace_tests;
#[cfg(test)]
mod tests;
