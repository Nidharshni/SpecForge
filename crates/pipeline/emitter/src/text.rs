//! Text an emitter writes. `fmt::Write` into a `String` fails only when a `Display` impl returns `Err`, which
//! no emitter formats: each writes through `?` and [`written`] turns the one result into the text.

use std::fmt;

/// The text `write` produces.
pub(crate) fn written(write: impl FnOnce(&mut String) -> fmt::Result) -> String {
    let mut out = String::new();
    write(&mut out)
        .expect("a String accepts every write, and no emitter formats a Display that fails");
    out
}
