//! Following what a project session is built from (ADR 0035 D2, ADR 0099): after an update that
//! moved the session's inputs, the watchers move to what the inputs ask for, the session catches
//! up on what changed while they did not watch, and again while that catch-up moves the inputs,
//! at most `FOLLOW_ROUNDS` times. Watch follows with directories, the LSP with its client's file
//! watchers.

/// How many rounds one follow runs at most: each needs the previous catch-up to have moved the
/// inputs again, so it ends long before this in practice.
const FOLLOW_ROUNDS: usize = 8;

/// What a surface follows the session's inputs with.
pub trait Follower {
    /// What is watched: watch's roots, the LSP's file watchers.
    type Watched: PartialEq + Clone;

    /// What the session's inputs ask to be watched now; `None`: nothing to follow (no session).
    fn wanted(&self) -> Option<Self::Watched>;

    /// Watch exactly `wanted` from now on; `Err` (why) when it is refused.
    fn watch(&mut self, wanted: &Self::Watched) -> Result<(), String>;

    /// `watch` refused for `reason`: report it, and return what is watched now: `was` (what was
    /// watched) unless the surface falls back to something else.
    fn refused(&mut self, reason: String, was: Self::Watched) -> Self::Watched;

    /// Bring the session up to date with what changed while the watchers moved: whether that
    /// moved the inputs again.
    fn catch_up(&mut self) -> bool;
}

/// What is watched, kept between follows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Following<W> {
    watched: W,
}

impl<W: PartialEq + Clone> Following<W> {
    /// Following with `watched` already watched.
    pub fn new(watched: W) -> Self {
        Self { watched }
    }

    /// What is watched.
    pub fn watched(&self) -> &W {
        &self.watched
    }

    /// Follow after an update that moved the inputs. A set refused once is not asked for again
    /// in this follow (the next update that moves the inputs asks again); after a refusal the
    /// session still catches up.
    pub fn follow<F: Follower<Watched = W>>(&mut self, follower: &mut F) {
        let mut refused: Option<W> = None;
        for _ in 0..FOLLOW_ROUNDS {
            let Some(wanted) = follower.wanted() else {
                return;
            };
            if wanted == self.watched || refused.as_ref() == Some(&wanted) {
                return; // every input is watched, or the watchers cannot take what is wanted
            }
            match follower.watch(&wanted) {
                Ok(()) => self.watched = wanted,
                Err(reason) => {
                    self.watched = follower.refused(reason, self.watched.clone());
                    refused = Some(wanted);
                }
            }
            if !follower.catch_up() {
                return;
            }
        }
    }
}
