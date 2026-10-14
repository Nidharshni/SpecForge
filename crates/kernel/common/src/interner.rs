//! Process-global string interner (`lasso::ThreadedRodeo` behind a
//! `LazyLock`). Contract:
//!
//! - **Leak by design**: interned strings live for `'static` (`Sym::as_str`
//!   returns `&'static str`) and are never freed. The interned set grows
//!   monotonically for the life of the process — fine for a compiler CLI,
//!   wrong primitive for a long-lived service.
//! - **Single writer**: `ThreadedRodeo` serializes interns behind one
//!   `RwLock`; reads (`resolve`) proceed concurrently.
//! - **Empty-string sentinel**: the empty string interns to a stable `Sym`;
//!   a default `Sym` resolves to it, so the zeroed symbol is a valid empty
//!   symbol, not a null.
use lasso::{Spur, ThreadedRodeo};
use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

static INTERNER: LazyLock<ThreadedRodeo> = LazyLock::new(ThreadedRodeo::default);

/// An interned string symbol. Wraps `lasso::Spur` (a `u32` index).
/// `Copy`, `Eq`, and `Hash` are all O(1). Resolves back to `&'static str`.
#[derive(Clone, Copy, Eq)]
pub struct Sym(Spur);

impl Sym {
    /// Intern a string and return its symbol.
    pub fn new(s: &str) -> Self {
        Self(INTERNER.get_or_intern(s))
    }

    /// Resolve the symbol back to its string.
    pub fn as_str(self) -> &'static str {
        INTERNER.resolve(&self.0)
    }

    /// Returns true if the interned string is empty.
    pub fn is_empty(self) -> bool {
        self.as_str().is_empty()
    }

    /// Returns the length of the interned string.
    pub fn len(self) -> usize {
        self.as_str().len()
    }
}

impl PartialEq for Sym {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<str> for Sym {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Sym {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Hash for Sym {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Debug for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sym({:?})", self.as_str())
    }
}

impl fmt::Display for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialOrd for Sym {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Sym {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl From<&str> for Sym {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Sym {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

impl From<Sym> for String {
    fn from(s: Sym) -> String {
        s.as_str().to_string()
    }
}

impl AsRef<str> for Sym {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for Sym {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Default for Sym {
    fn default() -> Self {
        Self::new("")
    }
}

impl serde::Serialize for Sym {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Sym {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Sym::new(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_roundtrip() {
        let s = Sym::new("hello");
        assert_eq!(s.as_str(), "hello");
    }

    #[test]
    fn same_string_same_sym() {
        let a = Sym::new("world");
        let b = Sym::new("world");
        assert_eq!(a, b);
    }

    #[test]
    fn different_strings_different_sym() {
        let a = Sym::new("foo");
        let b = Sym::new("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn sym_is_copy() {
        let a = Sym::new("copy_test");
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    #[test]
    fn compare_with_str() {
        let s = Sym::new("test");
        assert_eq!(s, *"test");
    }

    #[test]
    fn serialize_json() {
        let s = Sym::new("json_test");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"json_test\"");
    }

    #[test]
    fn deserialize_json() {
        let s: Sym = serde_json::from_str("\"deser_test\"").unwrap();
        assert_eq!(s.as_str(), "deser_test");
    }
}
