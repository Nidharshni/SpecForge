//! The versioned schema (ADR 0067): the Graph Protocol schema the loaded
//! extensions produce, with the version the schema record gives it (the
//! recorded version bumped by the largest change since), the changes, what
//! they report and its content hash. One version names one document.
//!
//! The record is `<root>/.specforge/schema-cache.json`. Only `specforge
//! export` writes it (ADR 0015 D10), after a successful export; every other
//! reader (`specforge schema`, `specforge.schema`, `specforge://schema`, the
//! MCP export and query tools, `schema --publish`) only versions against it.

mod diff;

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};
use specforge_emitter::{GraphProtocolSchema, SchemaVersion, content_hash};

pub use diff::SchemaChange;
pub(crate) use diff::negotiate_version;

const CACHE_FILE: &str = "schema-cache.json";

/// The directory under a project root that holds the record.
const CACHE_DIR: &str = ".specforge";

/// The schema record in one directory: `schema-cache.json`, the versioned
/// schema the last `specforge export` wrote, with its content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaCache {
    dir: PathBuf,
}

/// What the record file holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    schema: GraphProtocolSchema,
    content_hash: String,
}

impl SchemaCache {
    /// The record of the project at `root`: `<root>/.specforge/`.
    pub fn of_root(root: &Path) -> Self {
        Self::in_dir(root.join(CACHE_DIR))
    }

    /// The record in `dir`.
    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        SchemaCache { dir: dir.into() }
    }

    /// The directory holding `schema-cache.json`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `generated`, versioned against the record: the recorded version
    /// bumped by the changes since (major for a breaking change, minor for
    /// an addition, patch for anything else; unchanged when nothing
    /// changed). With no record, or one that cannot be read, it is 1.0.0.
    /// Reads the record once; writes nothing.
    pub fn version(&self, mut generated: GraphProtocolSchema) -> VersionedSchema {
        let previous = self.load().ok().flatten().map(|record| record.schema);
        let since = match previous {
            None => {
                generated.schema_version = SchemaVersion::new(1, 0, 0);
                Since::NoRecord
            }
            Some(recorded) => {
                let changes = diff::diff(&recorded, &generated);
                generated.schema_version = diff::bumped(&recorded.schema_version, &changes);
                Since::Recorded { changes }
            }
        };
        VersionedSchema::new(generated, since)
    }

    /// Replace the record with `versioned` and its content hash, atomically
    /// (a temporary file, then a rename). Only `specforge export` records.
    pub fn record(&self, versioned: &VersionedSchema) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;

        let record = Record {
            content_hash: versioned.content_hash().to_string(),
            schema: versioned.schema().clone(),
        };
        let json = serde_json::to_string_pretty(&record).map_err(io::Error::other)?;

        // Atomic write: temp file then rename
        let target = self.dir.join(CACHE_FILE);
        let tmp = self.dir.join(".schema-cache.tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, &target)?;

        Ok(())
    }

    /// What the previous export recorded, if anything.
    fn load(&self) -> io::Result<Option<Record>> {
        let path = self.dir.join(CACHE_FILE);
        if !path.exists() {
            return Ok(None);
        }

        let contents = std::fs::read_to_string(&path)?;
        let record: Record = serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Some(record))
    }
}

/// A Graph Protocol schema with its version and what changed since the
/// record it was versioned against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedSchema {
    schema: GraphProtocolSchema,
    content_hash: String,
    since: Since,
}

/// What the versioning was against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Since {
    NoRecord,
    Recorded { changes: Vec<SchemaChange> },
}

impl VersionedSchema {
    fn new(schema: GraphProtocolSchema, since: Since) -> Self {
        VersionedSchema {
            content_hash: content_hash(&schema),
            schema,
            since,
        }
    }

    /// A schema with no record to version against (a view with no root):
    /// 1.0.0, nothing changed.
    pub fn unrecorded(mut generated: GraphProtocolSchema) -> Self {
        generated.schema_version = SchemaVersion::new(1, 0, 0);
        Self::new(generated, Since::NoRecord)
    }

    /// The schema, carrying its version.
    pub fn schema(&self) -> &GraphProtocolSchema {
        &self.schema
    }

    pub fn into_schema(self) -> GraphProtocolSchema {
        self.schema
    }

    pub fn version(&self) -> &SchemaVersion {
        &self.schema.schema_version
    }

    /// SHA-256 of the schema's JSON ([`specforge_emitter::content_hash`]):
    /// what the record keeps, and what a scoped export's `schema_ref` names.
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// Each change since the recorded schema; empty with no record.
    pub fn changes(&self) -> &[SchemaChange] {
        match &self.since {
            Since::NoRecord => &[],
            Since::Recorded { changes } => changes,
        }
    }

    /// What versioning reports before an export writes: one W053 per
    /// breaking change. Empty otherwise.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.changes()
            .iter()
            .filter(|change| change.is_breaking())
            .map(|change| {
                Diagnostic::new(
                    codes::W053,
                    format!("breaking schema change since the last export: {change}"),
                )
                .with_suggestion(
                    "Agents and tools that read the previous export may not accept this one: \
                     update them, or keep the extension versions that produced the old schema."
                        .to_string(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
impl SchemaCache {
    /// Write `schema` as the record as it stands (its own version), for
    /// tests that seed an older record.
    pub(crate) fn seed(&self, schema: &GraphProtocolSchema) {
        self.record(&VersionedSchema::new(schema.clone(), Since::NoRecord))
            .unwrap();
    }
}

#[cfg(test)]
mod tests;
