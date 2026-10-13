//! Naming-convention linkage: tests a collector couldn't link to an entity
//! are linked by their names (`integrations/rust/spec` `convention_as_fallback`).
//!
//! Collectors are pure and never see the graph, so they return such tests
//! as *unlinked*; the host, which knows every entity and its obligations,
//! resolves them here before merging:
//!
//! 1. `add_item__rejects_a_duplicate_item`: the part before a `__` is an
//!    entity ID and the rest names one of its obligations.
//! 2. Otherwise the innermost enclosing module named after an entity
//!    (`mod add_item { fn rejects_a_duplicate_item() }`).
//!
//! The name after the entity proves an obligation when it's that
//! obligation's [`slug`]; otherwise the test is linked to the entity but
//! proves none of its obligations.
//! Tests no rule links are left out silently: plain tests are the norm.

use super::KnownEntities;
use specforge_common::{Diagnostic, codes, slug};
use specforge_protocol_types::{CollectEntityResult, CollectUnlinkedTest, TestResult};
use std::collections::BTreeMap;

/// Link `unlinked` tests to entities by naming convention. A name that
/// splits into more than one known entity is ambiguous (W137) and left out.
pub fn resolve(
    unlinked: &[CollectUnlinkedTest],
    known: &KnownEntities,
) -> (Vec<CollectEntityResult>, Vec<Diagnostic>) {
    let mut by_entity: BTreeMap<&str, Vec<TestResult>> = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for test in unlinked {
        let Some((name, modules)) = test.path.split_last() else {
            continue;
        };
        let split: Vec<(&str, &str)> = name
            .match_indices("__")
            .map(|(at, _)| (&name[..at], &name[at + 2..]))
            .filter(|(id, _)| known.contains(id))
            .collect();
        let (entity, rest) = match split.as_slice() {
            [one] => *one,
            [] => match modules.iter().rev().find(|m| known.contains(m)) {
                Some(module) => (module.as_str(), name.as_str()),
                None => continue,
            },
            several => {
                let ids: Vec<&str> = several.iter().map(|(id, _)| *id).collect();
                diagnostics.push(
                    Diagnostic::new(
                        codes::W137,
                        format!(
                            "test '{}' names several entities by convention ({}); it is not linked",
                            test.name,
                            ids.join(", ")
                        ),
                    )
                    .with_suggestion(
                        "rename the test, or link it with #[specforge_test]".to_string(),
                    ),
                );
                continue;
            }
        };
        by_entity.entry(entity).or_default().push(TestResult {
            name: test.name.clone(),
            status: test.status,
            verify: obligation(known.obligations(entity), rest),
            duration_ms: None,
        });
    }
    let results = by_entity
        .into_iter()
        .map(|(entity_id, test_results)| CollectEntityResult {
            entity_id: entity_id.to_string(),
            test_results,
        })
        .collect();
    (results, diagnostics)
}

/// The one obligation whose slug is `name`'s, if exactly one text has it.
fn obligation(texts: &[String], name: &str) -> Option<String> {
    let wanted = slug(name);
    let mut matching = texts.iter().filter(|text| slug(text) == wanted);
    let first = matching.next()?;
    matching.all(|t| t == first).then(|| first.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_common::Severity;
    use specforge_protocol_types::TestStatus;
    use specforge_test_macros::test as specforge_test;

    fn known() -> KnownEntities {
        KnownEntities::from_iter([
            (
                "add_item".to_string(),
                vec![
                    "rejects a duplicate item".to_string(),
                    "p99 < 200ms".to_string(),
                ],
            ),
            ("cart".to_string(), vec!["starts empty".to_string()]),
            ("cart_item".to_string(), vec![]),
            (
                "twice".to_string(),
                vec!["a < b".to_string(), "a lt b".to_string()],
            ),
        ])
    }

    fn test(name: &str, status: TestStatus) -> CollectUnlinkedTest {
        CollectUnlinkedTest {
            name: name.to_string(),
            path: name.split("::").map(str::to_string).collect(),
            status,
        }
    }

    /// `(entity, test name, verify)` for every linked test.
    fn linked(names: &[&str], known: &KnownEntities) -> Vec<(String, String, Option<String>)> {
        let unlinked: Vec<CollectUnlinkedTest> =
            names.iter().map(|n| test(n, TestStatus::Passed)).collect();
        let (results, diagnostics) = resolve(&unlinked, known);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        results
            .into_iter()
            .flat_map(|e| {
                e.test_results
                    .into_iter()
                    .map(move |t| (e.entity_id.clone(), t.name, t.verify))
            })
            .collect()
    }

    fn row(entity: &str, name: &str, verify: Option<&str>) -> (String, String, Option<String>) {
        (entity.into(), name.into(), verify.map(str::to_string))
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "a double underscore splits the entity from the obligation"
    )]
    fn a_double_underscore_splits_the_entity_from_the_obligation() {
        assert_eq!(
            linked(
                &[
                    "tests::add_item__rejects_a_duplicate_item",
                    "add_item__p99_lt_200ms",
                    "cart__works",
                ],
                &known()
            ),
            vec![
                row(
                    "add_item",
                    "tests::add_item__rejects_a_duplicate_item",
                    Some("rejects a duplicate item")
                ),
                row("add_item", "add_item__p99_lt_200ms", Some("p99 < 200ms")),
                row("cart", "cart__works", None),
            ],
            "a name that isn't an obligation's slug proves the entity alone"
        );
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "an entity with single underscores is not split"
    )]
    fn an_entity_with_single_underscores_is_not_split() {
        assert_eq!(
            linked(&["cart_item__is_counted", "cart_item_is_counted"], &known()),
            vec![row("cart_item", "cart_item__is_counted", None)],
            "only a double underscore separates"
        );
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "the innermost module named after an entity links its tests"
    )]
    fn the_innermost_module_named_after_an_entity_links_its_tests() {
        assert_eq!(
            linked(
                &[
                    "tests::add_item::rejects_a_duplicate_item",
                    "cart::add_item::starts_empty",
                    "cart::tests::starts_empty",
                    "cart__starts_empty::helper",
                ],
                &known()
            ),
            vec![
                row(
                    "add_item",
                    "tests::add_item::rejects_a_duplicate_item",
                    Some("rejects a duplicate item")
                ),
                row("add_item", "cart::add_item::starts_empty", None),
                row("cart", "cart::tests::starts_empty", Some("starts empty")),
            ],
            "the test's own name is never a module"
        );
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "a double underscore takes precedence over the module"
    )]
    fn a_double_underscore_takes_precedence_over_the_module() {
        assert_eq!(
            linked(&["cart::add_item__rejects_a_duplicate_item"], &known()),
            vec![row(
                "add_item",
                "cart::add_item__rejects_a_duplicate_item",
                Some("rejects a duplicate item")
            )]
        );
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "an obligation matches only when its slug is unique"
    )]
    fn an_obligation_matches_only_when_its_slug_is_unique() {
        assert_eq!(
            linked(&["twice__a_lt_b"], &known()),
            vec![row("twice", "twice__a_lt_b", None)],
            "'a < b' and 'a lt b' share a slug"
        );
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "a name that splits into several entities is W137"
    )]
    fn a_name_that_splits_into_several_entities_is_w137() {
        let known =
            KnownEntities::from_iter([("a".to_string(), vec![]), ("a__b".to_string(), vec![])]);
        let (results, diagnostics) = resolve(&[test("a__b__c", TestStatus::Passed)], &known);
        assert!(results.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "W137");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0].message.contains("a, a__b"));
    }

    #[specforge_test(
        behavior = "resolve_test_conventions",
        verify = "a test no convention links is left out silently"
    )]
    fn a_test_no_convention_links_is_left_out_silently() {
        let (results, diagnostics) = resolve(
            &[
                test("tests::unrelated", TestStatus::Failed),
                test("nobody__x", TestStatus::Passed),
            ],
            &known(),
        );
        assert!(results.is_empty());
        assert!(diagnostics.is_empty());
    }
}
