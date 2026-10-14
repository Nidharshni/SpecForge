//! `gate/baseline.json`: what the checks tolerate today, one section per check that keeps one. `specs`: per project,
//! the count of each warning code. A check fails when what it finds differs from its section in either direction;
//! `cargo gate baseline` lowers a section and never raises an entry.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where the baseline lives, from the repository root; named in the checks' messages.
pub const PATH: &str = "gate/baseline.json";

/// The checks that keep a section, as `cargo gate baseline --only` names them.
pub const SECTIONS: &[&str] = &["specs"];

/// Per project root (`.`, `examples/shop`), per warning code (`W004`): the count the `specs` check tolerates.
pub type WarningCounts = BTreeMap<String, BTreeMap<String, u64>>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    #[serde(default)]
    pub specs: WarningCounts,
}

impl Baseline {
    pub fn read(root: &Path) -> Result<Baseline, String> {
        let path = root.join(PATH);
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Pretty JSON, sections and keys sorted (the maps are `BTreeMap`s, whatever `serde_json`'s features), a final
    /// newline.
    pub fn write(&self, root: &Path) -> Result<(), String> {
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        let path = root.join(PATH);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_baseline_round_trips_sorted_and_refuses_an_unknown_section() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("gate")).unwrap();
        let baseline = Baseline {
            specs: WarningCounts::from([(".".into(), BTreeMap::from([("W003".into(), 1)]))]),
        };
        baseline.write(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(PATH)).unwrap();
        assert_eq!(
            text,
            "{\n  \"specs\": {\n    \".\": {\n      \"W003\": 1\n    }\n  }\n}\n"
        );
        assert_eq!(Baseline::read(dir.path()).unwrap(), baseline);

        std::fs::write(dir.path().join(PATH), "{}").unwrap();
        assert_eq!(Baseline::read(dir.path()).unwrap(), Baseline::default());

        // The pre-section shape (projects at the top) is not read as an empty baseline.
        std::fs::write(dir.path().join(PATH), r#"{".": {"W003": 1}}"#).unwrap();
        let refused = Baseline::read(dir.path()).unwrap_err();
        assert!(refused.contains("unknown field `.`"), "{refused}");
    }

    #[test]
    fn the_committed_baseline_reads() {
        let root = crate::repo::this_repo().root;
        Baseline::read(&root).unwrap();
    }
}
