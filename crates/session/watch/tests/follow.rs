//! The follow loop, without a session or a client: a scripted [`Follower`] over `u8` as what is
//! watched, recording the calls the loop makes in order.

use std::collections::VecDeque;

use specforge_test_macros::test as spec;
use specforge_watch::{Follower, Following};

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Watch(u8),
    Refused,
    CatchUp,
}

/// Answers `wanted` from a queue (the last value stays), refuses what `refuse` names and falls
/// back to `fallback`, answers each `catch_up` from a queue (`false` when it runs dry).
struct Script {
    wanted: VecDeque<u8>,
    none: bool,
    refuse: Option<u8>,
    fallback: Option<u8>,
    catch_ups: VecDeque<bool>,
    calls: Vec<Call>,
}

impl Script {
    fn wanting(wanted: &[u8]) -> Self {
        Script {
            wanted: wanted.iter().copied().collect(),
            none: false,
            refuse: None,
            fallback: None,
            catch_ups: VecDeque::new(),
            calls: Vec::new(),
        }
    }

    fn refusing(mut self, set: u8) -> Self {
        self.refuse = Some(set);
        self
    }

    fn falling_back_to(mut self, set: u8) -> Self {
        self.fallback = Some(set);
        self
    }

    fn catching_up(mut self, answers: &[bool]) -> Self {
        self.catch_ups = answers.iter().copied().collect();
        self
    }

    fn always_catching_up(self) -> Self {
        self.catching_up(&[true; 64])
    }
}

impl Follower for Script {
    type Watched = u8;

    fn wanted(&self) -> Option<u8> {
        if self.none {
            return None;
        }
        self.wanted.front().copied()
    }

    fn watch(&mut self, wanted: &u8) -> Result<(), String> {
        self.calls.push(Call::Watch(*wanted));
        if self.refuse == Some(*wanted) {
            Err("refused".into())
        } else {
            Ok(())
        }
    }

    fn refused(&mut self, _reason: String, was: u8) -> u8 {
        self.calls.push(Call::Refused);
        self.fallback.unwrap_or(was)
    }

    fn catch_up(&mut self) -> bool {
        self.calls.push(Call::CatchUp);
        // What the catch-up found moves what is wanted next, when the script has more to say.
        if self.wanted.len() > 1 {
            self.wanted.pop_front();
        }
        self.catch_ups.pop_front().unwrap_or(false)
    }
}

#[test]
fn an_unchanged_set_asks_for_nothing_and_catches_up_on_nothing() {
    let mut script = Script::wanting(&[1]).always_catching_up();
    let mut following = Following::new(1);
    following.follow(&mut script);
    assert_eq!(script.calls, vec![]);
}

#[test]
fn a_moved_set_is_watched_then_caught_up() {
    let mut script = Script::wanting(&[2]);
    let mut following = Following::new(1);
    following.follow(&mut script);
    assert_eq!(script.calls, vec![Call::Watch(2), Call::CatchUp]);
    assert_eq!(following.watched(), &2);
}

#[spec(
    behavior = "classify_project_changes",
    verify = "a set of watchers refused once is not asked for again in the same follow"
)]
fn a_set_refused_once_is_not_asked_for_again_in_the_follow() {
    let mut script = Script::wanting(&[2]).refusing(2).always_catching_up();
    let mut following = Following::new(1);
    following.follow(&mut script);
    assert_eq!(
        script.calls,
        vec![Call::Watch(2), Call::Refused, Call::CatchUp]
    );
}

#[test]
fn a_refusal_keeps_what_the_follower_returns_and_still_catches_up() {
    let mut script = Script::wanting(&[2]).refusing(2).falling_back_to(9);
    let mut following = Following::new(1);
    following.follow(&mut script);
    assert_eq!(following.watched(), &9);
    assert_eq!(
        script.calls.iter().filter(|c| **c == Call::CatchUp).count(),
        1
    );
}

#[test]
fn a_later_follow_asks_for_a_refused_set_again() {
    let mut script = Script::wanting(&[2]).refusing(2);
    let mut following = Following::new(1);
    following.follow(&mut script);
    following.follow(&mut script);
    let asks = script
        .calls
        .iter()
        .filter(|c| **c == Call::Watch(2))
        .count();
    assert_eq!(asks, 2);
}

#[test]
fn a_follow_ends_after_eight_rounds() {
    // What is wanted changes every round and every catch-up moves the inputs again.
    let wanted: Vec<u8> = (2..=40).collect();
    let mut script = Script::wanting(&wanted).always_catching_up();
    let mut following = Following::new(1);
    following.follow(&mut script);
    let asks = script
        .calls
        .iter()
        .filter(|c| matches!(c, Call::Watch(_)))
        .count();
    assert_eq!(asks, 8);
}

#[test]
fn nothing_wanted_ends_the_follow() {
    let mut script = Script::wanting(&[2]).always_catching_up();
    script.none = true;
    let mut following = Following::new(1);
    following.follow(&mut script);
    assert_eq!(script.calls, vec![]);
    assert_eq!(following.watched(), &1);
}
