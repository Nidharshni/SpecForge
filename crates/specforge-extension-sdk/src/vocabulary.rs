//! A closed field vocabulary (ADR 0071): the values a field takes, in
//! their order, and the value an absent field counts as, declared once.

/// A closed set of values a field takes, in their order, and the value an
/// absent field counts as. An extension declares it once, as a `const`, and
/// everything that needs the values reads it:
///
/// - the field's declaration ([`FieldBuilder::vocabulary`]): an `enum` field
///   whose `enum_values` are these values and whose `default_value` is the
///   absent value, so the host refuses another value (E061, naming the
///   closest), completes the values in the editor, lists them in the schema
///   and the published JSON Schema, and shows them and the default in the
///   guide and the hover;
/// - a command arg that takes one of them (`ArgBuilder::one_of(v.values())`);
/// - the extension's own queries and passes ([`Self::read`],
///   [`Self::position`]) and the values its rules name ([`Self::member`]).
///
/// Every check runs when the constant is evaluated, so a vocabulary that
/// contradicts itself, or a member name it does not hold, does not compile.
///
/// [`FieldBuilder::vocabulary`]: crate::FieldBuilder::vocabulary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vocabulary {
    values: &'static [&'static str],
    absent_as: Option<&'static str>,
}

/// Whether `a` and `b` are the same text (`==` is not `const` on `str`).
const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

impl Vocabulary {
    /// The vocabulary of `values`, in order, with no absent value. Panics (a
    /// compile error in a `const`) when `values` is empty, holds an empty
    /// value, or holds a value twice.
    pub const fn new(values: &'static [&'static str]) -> Self {
        assert!(!values.is_empty(), "a vocabulary holds at least one value");
        let mut i = 0;
        while i < values.len() {
            assert!(!values[i].is_empty(), "a vocabulary value is not empty");
            let mut j = i + 1;
            while j < values.len() {
                assert!(
                    !same(values[i], values[j]),
                    "a vocabulary value is held once"
                );
                j += 1;
            }
            i += 1;
        }
        Self {
            values,
            absent_as: None,
        }
    }

    /// This vocabulary, with an absent field counting as `value`. Panics when
    /// `value` is not one of its values.
    pub const fn absent_as(self, value: &'static str) -> Self {
        Self {
            values: self.values,
            absent_as: Some(self.member(value)),
        }
    }

    /// Its values, in order.
    pub const fn values(&self) -> &'static [&'static str] {
        self.values
    }

    /// The value an absent field counts as, if any.
    pub const fn absent(&self) -> Option<&'static str> {
        self.absent_as
    }

    /// `value`, which must be one of its values: name a member in a `const`
    /// (`const DONE: &str = FEATURE_STATUS.member("done");`) so a misspelt
    /// one does not compile. Panics otherwise.
    pub const fn member(&self, value: &str) -> &'static str {
        self.values[self.index(value)]
    }

    /// The place of `member` in the order; panics when it is not a member.
    pub const fn index(&self, member: &str) -> usize {
        let mut i = 0;
        while i < self.values.len() {
            if same(self.values[i], member) {
                return i;
            }
            i += 1;
        }
        panic!("not a member of the vocabulary")
    }

    /// The place of `value` in the order (a sort's rank); `None` outside it.
    pub fn position(&self, value: &str) -> Option<usize> {
        self.values.iter().position(|v| *v == value)
    }

    /// Whether `value` is one of its values.
    pub fn contains(&self, value: &str) -> bool {
        self.position(value).is_some()
    }

    /// What a field reads as: its written value, or, when it is absent or
    /// empty, the absent value (`None` when the vocabulary has none).
    pub fn read<'a>(&self, written: Option<&'a str>) -> Option<&'a str> {
        match written {
            Some(value) if !value.is_empty() => Some(value),
            _ => self.absent_as,
        }
    }
}

#[cfg(test)]
mod tests;
