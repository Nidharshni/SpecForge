use std::path::Path;

pub trait ReportWriter {
    fn write_report(&self, path: &Path, content: &str) -> std::io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
    /// Append one line (no trailing newline handling beyond the caller's) —
    /// the durability channel for per-test JSONL records (C11-06).
    fn append_line(&self, path: &Path, line: &str) -> std::io::Result<()>;
}

pub struct RealFs;

impl ReportWriter for RealFs {
    fn write_report(&self, path: &Path, content: &str) -> std::io::Result<()> {
        std::fs::write(path, content)
    }

    fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn append_line(&self, path: &Path, line: &str) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{line}")
    }
}
