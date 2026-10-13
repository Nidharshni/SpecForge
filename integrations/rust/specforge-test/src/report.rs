use crate::registry::TestRecordEntry;
use serde::Serialize;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Wire schema version of the report JSON (`schema/specforge-binary-report.schema.json`).
pub const REPORT_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Serialize)]
struct BinaryReport<'a> {
    schema_version: &'a str,
    binary_name: &'a str,
    entries: &'a [TestRecordEntry],
}

/// Write `<dir>/<binary_name>.json`: the entries sorted by entity and test
/// name, as compact JSON, through a temporary file and a rename so a crash or
/// a full disk never leaves a truncated report. No entries, no file.
pub fn write_report(
    dir: &Path,
    binary_name: &str,
    entries: &[TestRecordEntry],
) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| {
        a.entity_id
            .cmp(&b.entity_id)
            .then(a.test_name.cmp(&b.test_name))
    });
    let report = BinaryReport {
        schema_version: REPORT_SCHEMA_VERSION,
        binary_name,
        entries: &sorted,
    };

    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{binary_name}.json"));
    // C6-08: temp-file-plus-rename, same policy as persist_schema_cache.
    let tmp = dir.join(format!(".{binary_name}.json.tmp"));
    // C6-15: compact JSON streamed into a buffered file writer, no
    // intermediate String and no pretty-print whitespace.
    let mut writer = BufWriter::new(std::fs::File::create(&tmp)?);
    serde_json::to_writer(&mut writer, &report).map_err(std::io::Error::other)?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(tmp, path)
}
