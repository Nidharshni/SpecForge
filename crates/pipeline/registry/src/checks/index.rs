//! The indexes bundled with SpecForge that name the builtin extension to
//! install: one for entity keywords (E024), one for the fields builtin
//! extensions add to other extensions' kinds (W020).

use std::collections::HashMap;

/// A keyword-to-extension index for suggesting missing extensions. Loaded
/// lazily from a bundled data file on first use.
#[derive(Debug, Default)]
pub(crate) struct KeywordExtensionIndex {
    entries: HashMap<String, String>,
}

impl KeywordExtensionIndex {
    pub(crate) fn lookup(&self, keyword: &str) -> Option<&str> {
        self.entries.get(keyword).map(|s| s.as_str())
    }

    /// `{"keyword": "@scope/extension"}`; malformed data gives an empty
    /// index, so suggestions fall back to `specforge search`.
    fn from_json(json: &str) -> Self {
        Self {
            entries: serde_json::from_str(json).unwrap_or_default(),
        }
    }

    /// The index shipped with SpecForge (`data/keyword-index.json`): each
    /// builtin extension's entity keywords. Parsed on first use.
    pub(crate) fn bundled() -> &'static KeywordExtensionIndex {
        static BUNDLED: std::sync::OnceLock<KeywordExtensionIndex> = std::sync::OnceLock::new();
        BUNDLED.get_or_init(|| Self::from_json(include_str!("../../data/keyword-index.json")))
    }

    /// The fields builtin extensions add to other extensions' kinds
    /// (`data/field-index.json`, keyed `<kind>.<field>`), so W020 can name
    /// the extension that declares a field the project lacks, as E024
    /// does for a kind. Parsed on first use.
    pub(crate) fn bundled_fields() -> &'static KeywordExtensionIndex {
        static BUNDLED: std::sync::OnceLock<KeywordExtensionIndex> = std::sync::OnceLock::new();
        BUNDLED.get_or_init(|| Self::from_json(include_str!("../../data/field-index.json")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as spec;

    #[spec(
        behavior = "suggest_missing_extensions",
        verify = "keyword-to-extension index is loaded from bundled data file"
    )]
    fn the_bundled_keyword_index_maps_every_builtin_keyword() {
        // The bundled file must say what the builtins' own declarations say
        // (their pinned wire answers).
        let declarations =
            specforge_repo_root::repo_root().join("crates/runtime/component/tests/declarations");
        let mut expected = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(&declarations).unwrap() {
            let src = entry.unwrap().path();
            let Ok(entities) = std::fs::read_to_string(src.join("describe_entities.json")) else {
                continue;
            };
            let handshake: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(src.join("handshake.json")).unwrap())
                    .unwrap();
            let name = handshake["name"].as_str().unwrap().to_string();
            // The SDK greet fixture is pinned beside the builtins.
            if !name.starts_with("@specforge/") {
                continue;
            }
            let entities: serde_json::Value = serde_json::from_str(&entities).unwrap();
            for item in entities["items"].as_array().unwrap() {
                expected.insert(item["keyword"].as_str().unwrap().to_string(), name.clone());
            }
        }
        assert!(!expected.is_empty());

        let bundled = KeywordExtensionIndex::bundled();
        for (keyword, extension) in &expected {
            assert_eq!(
                bundled.lookup(keyword),
                Some(extension.as_str()),
                "{keyword}"
            );
        }
        assert_eq!(bundled.lookup("xyzzy"), None);
    }

    #[test]
    fn the_bundled_field_index_maps_every_builtin_enhancement_field() {
        // The bundled file must say what the builtins' enhancements say
        // (their pinned wire answers).
        let declarations =
            specforge_repo_root::repo_root().join("crates/runtime/component/tests/declarations");
        let mut expected = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(&declarations).unwrap() {
            let src = entry.unwrap().path();
            let Ok(enhancements) = std::fs::read_to_string(src.join("describe_enhancements.json"))
            else {
                continue;
            };
            let handshake: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(src.join("handshake.json")).unwrap())
                    .unwrap();
            let name = handshake["name"].as_str().unwrap().to_string();
            // The SDK greet fixture is pinned beside the builtins.
            if !name.starts_with("@specforge/") {
                continue;
            }
            let enhancements: serde_json::Value = serde_json::from_str(&enhancements).unwrap();
            for item in enhancements["items"].as_array().unwrap() {
                let kind = item["target_kind"].as_str().unwrap();
                for field in item["fields"].as_array().into_iter().flatten() {
                    let field = field["name"].as_str().unwrap();
                    expected.insert(format!("{kind}.{field}"), name.clone());
                }
            }
        }
        assert!(expected.contains_key("invariant.expression"));
        let bundled_json: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../data/field-index.json")).unwrap();
        assert_eq!(bundled_json, expected);

        let bundled = KeywordExtensionIndex::bundled_fields();
        for (key, extension) in &expected {
            assert_eq!(bundled.lookup(key), Some(extension.as_str()), "{key}");
        }
    }

    #[test]
    fn a_malformed_index_is_empty_so_suggestions_fall_back_to_search() {
        assert_eq!(
            KeywordExtensionIndex::from_json("{not json").lookup("feature"),
            None
        );
    }
}
