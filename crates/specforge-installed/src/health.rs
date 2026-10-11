use crate::Installed;
use crate::load::LoadProblem;
use crate::lock::LockFileEntry;

/// One problem with one lock entry (doctor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// The entry's module is not on disk.
    MissingModule { name: String },
    /// The entry's module is not the one the lock pins.
    Changed {
        name: String,
        locked: String,
        actual: String,
    },
}

impl Installed {
    /// Every lock entry checked: its module present and the one the lock pins. Healthy entries are
    /// absent. Peers are the compile's (ADR 0041).
    pub fn health(&self) -> Vec<Health> {
        self.lock()
            .entries()
            .iter()
            .filter_map(|entry| self.module_health(entry))
            .collect()
    }

    /// What is wrong with `entry`'s module, if anything: the check the
    /// load makes, so the two cannot disagree.
    fn module_health(&self, entry: &LockFileEntry) -> Option<Health> {
        let name = entry.name.to_string();
        match self.check_module(entry) {
            Ok(_) => None,
            Err(LoadProblem::ModuleMissing { .. }) => Some(Health::MissingModule { name }),
            Err(LoadProblem::Changed { locked, actual }) => Some(Health::Changed {
                name,
                locked,
                actual,
            }),
            // A module that cannot be read is not judged here: loading it
            // says why.
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::{LockFile, LockState};
    use crate::module::hex_sha256;
    use specforge_package::PackageName;
    use tempfile::TempDir;

    fn entry(name: &str, hash: &str) -> LockFileEntry {
        LockFileEntry {
            name: specforge_package::PackageName::parse(name).unwrap(),
            version: "1.0.0".to_string(),
            source: crate::LockSource::parse("registry"),
            wasm_hash: hash.to_string(),
            key_id: None,
            peer_dependencies: Vec::new(),
        }
    }

    fn installed(dir: &TempDir, entries: Vec<LockFileEntry>) -> Installed {
        Installed::with_lock(
            dir.path(),
            LockState::Read(LockFile {
                entries,
                ..Default::default()
            }),
        )
    }

    /// `name`'s module on disk under `dir`, holding `bytes`.
    fn put_module(installed: &Installed, name: &str, bytes: &[u8]) {
        let path = installed.module_path(&PackageName::parse(name).unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    // B:run_doctor_check — verify unit "detects missing binary"
    #[test]
    fn test_doctor_detects_missing_binary() {
        let dir = TempDir::new().unwrap();
        let installed = installed(&dir, vec![entry("missing-ext", "abc")]);

        assert_eq!(
            installed.health(),
            [Health::MissingModule {
                name: "missing-ext".to_string()
            }]
        );
    }

    // B:run_doctor_check — verify unit "detects stale hash"
    #[test]
    fn test_doctor_detects_stale_hash() {
        let dir = TempDir::new().unwrap();
        let installed = installed(&dir, vec![entry("my-ext", "expected_hash")]);
        put_module(&installed, "my-ext", b"wasm content");

        let problems = installed.health();

        assert_eq!(
            problems,
            [Health::Changed {
                name: "my-ext".to_string(),
                locked: "expected_hash".to_string(),
                actual: hex_sha256(b"wasm content"),
            }]
        );
    }

    // B:run_doctor_check — verify unit "reports healthy when all checks pass"
    #[test]
    fn test_doctor_reports_healthy() {
        let dir = TempDir::new().unwrap();
        let installed = installed(&dir, vec![entry("good-ext", &hex_sha256(b"wasm"))]);
        put_module(&installed, "good-ext", b"wasm");

        assert_eq!(installed.health(), []);
    }
}
