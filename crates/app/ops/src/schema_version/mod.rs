//! The versioned schema (ADR 0067): the Graph Protocol schema the loaded
//! extensions produce, with the version the schema record gives it (the
//! recorded version bumped by the largest change since), the changes, what
//! they report and its content hash. One version names one document.
//!
//! The record is `<root>/.specforge/schema-cache.json`. Only `specforge
//! export` writes it (ADR 0015 D10), after a successful export; every other
//! reader (`specforge schema`, `specforge.schema`, `specforge://schema`, the
//! MCP export and query tools, `schema --publish`) only versions against it.
//! Every reader reports what versioning found: [`VersionedSchema::notices`]
//! rides on each read outcome (ADR 0111).

mod diff;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};
use specforge_emitter::{GraphProtocolSchema, SchemaVersion, content_hash};

pub use diff::{AttributeChange, Bump, Members, Scalar, SchemaChange};

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
    /// changed). With no record it is 1.0.0 with no changes; with a record
    /// that cannot be read it is 1.0.0 and [`VersionedSchema::diagnostics`]
    /// holds I016. Kinds, fields and edge types are compared by name,
    /// whatever order the file holds them in. Reads the record once; writes
    /// nothing.
    pub fn version(&self, mut generated: GraphProtocolSchema) -> VersionedSchema {
        let since = match self.load() {
            Ok(None) => {
                generated.schema_version = SchemaVersion::new(1, 0, 0);
                Since::NoRecord
            }
            Err(error) => {
                generated.schema_version = SchemaVersion::new(1, 0, 0);
                Since::Unreadable {
                    path: self.dir.join(CACHE_FILE),
                    reason: error.to_string(),
                }
            }
            Ok(Some(Record {
                schema: recorded, ..
            })) => {
                let changes = diff::diff(&recorded, &generated);
                debug_assert_eq!(
                    changes.is_empty(),
                    canonical(&recorded) == canonical(&generated),
                    "the diff compares every attribute: {changes:?}"
                );
                generated.schema_version = diff::bumped(&recorded.schema_version, &changes);
                Since::Recorded {
                    previous: recorded.schema_version,
                    changes,
                }
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

/// A requested schema version whose readers do not read this schema: another
/// major, or a newer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAdmitted {
    pub requested: SchemaVersion,
    pub current: SchemaVersion,
}

impl NotAdmitted {
    /// The refusal's code: `incompatible_schema_version` (an operation code,
    /// not a catalogued diagnostic).
    pub const CODE: &'static str = "incompatible_schema_version";
}

impl fmt::Display for NotAdmitted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let NotAdmitted { requested, current } = self;
        let why = if requested.major != current.major {
            format!("has another major version: this project's schema is {current}")
        } else {
            format!("is newer than this project's schema {current}")
        };
        write!(
            f,
            "requested schema version {requested} {why}, which readers of {}.0.0 to {current} read",
            current.major
        )
    }
}

/// What the versioning was against.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Since {
    NoRecord,
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    Recorded {
        previous: SchemaVersion,
        changes: Vec<SchemaChange>,
    },
}

/// `schema` as the diff sees it: no version, kinds, fields and edge types in
/// name order, an empty set as an absent one.
fn canonical(schema: &GraphProtocolSchema) -> GraphProtocolSchema {
    let mut schema = schema.clone();
    schema.schema_version = SchemaVersion::new(0, 0, 0);
    schema.entity_kinds.sort_by(|a, b| a.name.cmp(&b.name));
    for kind in &mut schema.entity_kinds {
        kind.fields.sort_by(|a, b| a.name.cmp(&b.name));
        for field in &mut kind.fields {
            field.enum_values = field.enum_values.take().filter(|v| !v.is_empty());
        }
    }
    schema.edge_types.sort_by(|a, b| a.label.cmp(&b.label));
    for edge in &mut schema.edge_types {
        edge.source_kinds = edge.source_kinds.take().filter(|v| !v.is_empty());
        edge.target_kinds = edge.target_kinds.take().filter(|v| !v.is_empty());
    }
    schema
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

    /// Each change since the recorded schema, in schema order; empty with no
    /// record or an unreadable one.
    pub fn changes(&self) -> &[SchemaChange] {
        match &self.since {
            Since::NoRecord | Since::Unreadable { .. } => &[],
            Since::Recorded { changes, .. } => changes,
        }
    }

    /// Whether a reader of `requested` reads this schema's documents: the
    /// same major version, and not newer (`requested <= version`, pre-release
    /// labels ordered as [`SchemaVersion`]'s `Ord`).
    pub fn admits(&self, requested: &SchemaVersion) -> Result<(), NotAdmitted> {
        let current = self.version();
        if requested.major == current.major && requested <= current {
            Ok(())
        } else {
            Err(NotAdmitted {
                requested: requested.clone(),
                current: current.clone(),
            })
        }
    }

    /// What versioning found, for every read outcome that names the version
    /// to carry (ADR 0111): one W053 per change whose [`Bump`] is major, in
    /// [`Self::changes`] order, or the one I016 of an unreadable record.
    /// Empty otherwise. Crate-private: only a read turns versioning into
    /// notices, and hands them to a surface as a `Noticed` (ADR 0130).
    pub(crate) fn notices(&self) -> Vec<Diagnostic> {
        match &self.since {
            Since::NoRecord => Vec::new(),
            Since::Unreadable { path, reason } => vec![
                Diagnostic::new(
                    codes::I016,
                    format!(
                        "the schema cache {} could not be read ({reason}): the schema version \
                         starts again at 1.0.0",
                        path.display()
                    ),
                )
                .with_suggestion(
                    "The next `specforge export` records a new cache over it; until then every \
                     reader answers 1.0.0, and agents that read earlier exports must not take \
                     the version as continuing theirs."
                        .to_string(),
                ),
            ],
            Since::Recorded { previous, changes } => {
                let current = self.version();
                changes
                    .iter()
                    .filter(|change| change.bump() == Bump::Major)
                    .map(|change| {
                        Diagnostic::new(
                            codes::W053,
                            format!("breaking schema change since the last export: {change}"),
                        )
                        .with_suggestion(format!(
                            "The schema version goes from {previous} to {current}: agents and \
                             tools that read {}.x exports may not read this one. Update them, or \
                             keep the extension versions that produced {previous}.",
                            previous.major
                        ))
                    })
                    .collect()
            }
        }
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
