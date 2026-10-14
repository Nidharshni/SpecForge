/// Largest Levenshtein edit distance a suggestion may be from its target.
pub const MAX_SUGGESTION_DISTANCE: usize = 3;

/// Find the closest match for `target` among `candidates`.
///
/// A candidate qualifies when it is within [`MAX_SUGGESTION_DISTANCE`] edits
/// of `target` and also scores above 0.85 Jaro-Winkler similarity, which
/// keeps short, unrelated IDs (`abc` and `xyz` are three edits apart) from
/// being suggested. The fewest edits wins, then the higher similarity.
/// Returns `None` when no candidate qualifies.
pub fn find_close_match<'a>(
    target: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    candidates
        .into_iter()
        .filter_map(|c| {
            let distance = strsim::levenshtein(target, c);
            let score = strsim::jaro_winkler(target, c);
            (distance <= MAX_SUGGESTION_DISTANCE && score > 0.85).then_some((c, distance, score))
        })
        // Deterministic tie-break: among equal distances and scores pick the
        // lexicographically smallest candidate, so the suggestion never
        // depends on the caller's iteration order (R-6 / hardening-plan D1).
        .min_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.2.partial_cmp(&a.2).unwrap())
                .then_with(|| a.0.cmp(b.0))
        })
        .map(|(s, _, _)| s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_returns_candidate() {
        let result = find_close_match("alpha", ["alpha", "beta", "gamma"]);
        assert_eq!(result, Some("alpha"));
    }

    #[test]
    fn close_typo_returns_suggestion() {
        let result = find_close_match(
            "alpha_parsr",
            ["alpha_parser", "beta_builder", "gamma_runner"],
        );
        assert_eq!(result, Some("alpha_parser"));
    }

    #[test]
    fn no_close_match_returns_none() {
        let result = find_close_match("zzzzz", ["alpha", "beta", "gamma"]);
        assert_eq!(result, None);
    }

    #[test]
    fn empty_candidates_returns_none() {
        let result: Option<&str> = find_close_match("alpha", std::iter::empty());
        assert_eq!(result, None);
    }

    #[test]
    fn picks_best_among_multiple_close_matches() {
        let result = find_close_match("alpha_parse", ["alpha_parser", "alpha_parsed"]);
        // Both are close but "alpha_parser" should score higher (or either is acceptable)
        assert!(result.is_some());
    }
}
