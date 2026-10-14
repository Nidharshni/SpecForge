//! What a run that judges the project concluded (CONTEXT "Run verdict", ADR 0029 D3/D4).

/// What a run that judges the project concluded (CONTEXT "Run verdict",
/// ADR 0029 D3/D4): passed, failed, or it could not judge (a measuring
/// operation that lacks what it measures against). The CLI exits 0, 1 or
/// 2 by it; MCP returns `ok` as `verdict == Passed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunVerdict {
    Passed,
    Failed,
    Unjudged,
}

impl RunVerdict {
    /// `Passed` when `ok`, else `Failed`.
    pub const fn of(ok: bool) -> Self {
        if ok { Self::Passed } else { Self::Failed }
    }
}
