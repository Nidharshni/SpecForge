//! What a registry search matches (ADR 0044): one rule, shared by the registry server and both
//! in-memory registries.

/// Whether a package matches a registry search for `query`: an ASCII case-insensitive substring of
/// its name, its description or one of its keywords. An empty query matches every package;
/// `%` and `_` are text.
pub fn matches(query: &str, name: &str, description: &str, keywords: &[&str]) -> bool {
    let needle = query.to_ascii_lowercase();
    let has = |text: &str| text.to_ascii_lowercase().contains(&needle);
    has(name) || has(description) || keywords.iter().any(|keyword| has(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "search_registry",
        verify = "a search matches a case-insensitive substring of a package's name, description or one keyword, wildcards included as text"
    )]
    fn a_query_matches_a_substring_of_the_name_description_or_one_keyword() {
        assert!(matches("XT", "@a/myxext", "", &[]));
        assert!(matches("plain", "@a/x", "A Plain package", &[]));
        assert!(matches("BET", "@a/x", "", &["alpha", "beta"]));
        assert!(!matches("my_ext", "@a/myxext", "", &[]));
        assert!(!matches("a,b", "@a/x", "", &["alpha", "beta"]));
        assert!(matches("", "@a/x", "", &[]));
        assert!(!matches("%", "@a/x", "plain", &["alpha"]));
        assert!(matches("%", "@a/100%", "", &[]));
    }
}
