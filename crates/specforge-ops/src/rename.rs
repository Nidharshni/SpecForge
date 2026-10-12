//! Renaming an entity ID: the MCP `specforge.rename` tool and the LSP's
//! `textDocument/rename` plan the same edits under the same rules. MCP
//! applies them ([`apply`]) and recompiles; the LSP hands them to the
//! editor. The edits are exactly the occurrences navigation finds (the
//! declaration's name and each reference's token): text in strings,
//! comments and verify statements that mentions the ID is not a reference,
//! and is left alone (ADR 0016).

use crate::navigate::{Direction, Navigator, Precision, ReferenceQuery};
use crate::{OpError, OpErrorKind};
use std::collections::BTreeSet;
use std::path::Path;

/// The new ID is not a legal entity ID.
pub const INVALID_ID: &str = "invalid_name";
/// The new ID already names an entity.
pub const TAKEN: &str = "entity_exists";
/// A file the rename must edit cannot be read (or written).
pub const UNREADABLE: &str = "file_unreadable";

/// The shortest and longest entity IDs (E014).
pub const ID_LENGTH: std::ops::RangeInclusive<usize> = 2..=60;

/// Whether `id` is a legal entity ID: what the grammar's identifier accepts
/// (an ASCII letter or `_`, then letters, digits and `_`) within E014's
/// 2-60 characters.
pub fn validate_id(id: &str) -> Result<(), OpError> {
    let mut chars = id.chars();
    let starts = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let rest = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if starts && rest && ID_LENGTH.contains(&id.len()) {
        return Ok(());
    }
    Err(OpError::new(
        OpErrorKind::InvalidInput,
        INVALID_ID,
        format!(
            "invalid entity ID '{id}': it must be 2-60 characters, letters, digits and \
             underscores, starting with a letter or underscore"
        ),
    ))
}

/// A text edit for a rename operation: `start_col..end_col` (0-based
/// byte columns) on the 1-based `line` of `file` becomes `new_text`.
#[derive(Debug, Clone)]
pub struct RenameEdit {
    pub file: String,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub new_text: String,
}

/// `text` with `edits` applied: each replaces its byte range on its 1-based
/// line. The edits must all belong to `text`'s file (callers filter by
/// `file`); an edit whose line is past the end is ignored.
fn apply_edits<'a>(text: &str, edits: impl IntoIterator<Item = &'a RenameEdit>) -> String {
    let mut by_line: std::collections::BTreeMap<usize, Vec<&RenameEdit>> =
        std::collections::BTreeMap::new();
    for edit in edits {
        by_line.entry(edit.line).or_default().push(edit);
    }
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let Some(line_edits) = by_line.get_mut(&(index + 1)) else {
            out.push_str(line);
            continue;
        };
        // Right to left, so earlier columns stay valid.
        line_edits.sort_by_key(|e| std::cmp::Reverse(e.start_col));
        let mut line = line.to_string();
        for edit in line_edits.iter() {
            line.replace_range(edit.start_col..edit.end_col, &edit.new_text);
        }
        out.push_str(&line);
    }
    out
}

/// The edits that rename one entity: its declaration and every reference.
#[derive(Debug, Clone)]
pub struct RenamePlan {
    pub old_id: String,
    pub new_id: String,
    /// One per occurrence; files are relative to the spec root.
    pub edits: Vec<RenameEdit>,
}

impl RenamePlan {
    /// The files the edits touch, sorted.
    pub fn affected_files(&self) -> BTreeSet<&str> {
        self.edits.iter().map(|e| e.file.as_str()).collect()
    }
}

/// Plan renaming `old_id` to `new_id`: one edit per occurrence `nav`
/// finds, the declaration's name and each incoming reference's token,
/// each file read through the navigator. A rename is all or nothing: one
/// that cannot see every token (a file it cannot read, or whose text no
/// longer spells the ID where the graph says) is refused, rather than
/// guessing or leaving a reference behind.
pub fn plan<F: Fn(&str) -> Option<String>>(
    nav: &Navigator<'_, F>,
    old_id: &str,
    new_id: &str,
) -> Result<RenamePlan, OpError> {
    validate_id(new_id)?;
    let graph = nav.view().graph();
    if graph.node(old_id).is_none() {
        return Err(crate::navigate::not_found(graph, old_id));
    }
    if graph.node(new_id).is_some() {
        return Err(OpError::new(
            OpErrorKind::Conflict,
            TAKEN,
            format!("cannot rename '{old_id}': '{new_id}' exists"),
        )
        .with_entity(old_id));
    }
    let query = ReferenceQuery {
        direction: Direction::Incoming,
        include_declaration: true,
    };
    let occurrences = nav.references(old_id, query)?;
    let unreadable: BTreeSet<&str> = occurrences
        .iter()
        .filter(|o| o.precision == Precision::Entity)
        .map(|o| o.span.file.as_str())
        .collect();
    if !unreadable.is_empty() {
        let files: Vec<&str> = unreadable.into_iter().collect();
        return Err(OpError::new(
            OpErrorKind::Internal,
            UNREADABLE,
            format!("cannot rename '{old_id}': cannot read {}", files.join(", ")),
        ));
    }
    // A token is one line; RenameEdit columns are 0-based bytes.
    let edits = occurrences
        .iter()
        .map(|o| RenameEdit {
            file: o.span.file.to_string(),
            line: o.span.start_line,
            start_col: o.span.start_col - 1,
            end_col: o.span.end_col - 1,
            new_text: new_id.to_string(),
        })
        .collect();
    Ok(RenamePlan {
        old_id: old_id.to_string(),
        new_id: new_id.to_string(),
        edits,
    })
}

/// Write `plan`'s edits to the files under `spec_root`: every file is
/// read and edited first, then written; if a write fails, the files
/// already written, and the one that failed part-way, get their old text
/// back (and nothing is reported written). Returns the files it wrote.
pub fn apply(plan: &RenamePlan, spec_root: &Path) -> Result<crate::Writes, OpError> {
    let mut changes = Vec::new();
    for file in plan.affected_files() {
        let path = spec_root.join(file);
        let old = std::fs::read_to_string(&path).map_err(|e| {
            OpError::new(
                OpErrorKind::of_io(&e),
                UNREADABLE,
                format!("failed to read {}: {e}", path.display()),
            )
        })?;
        let new = apply_edits(&old, plan.edits.iter().filter(|e| e.file == file));
        changes.push((path, old, new));
    }
    for (i, (path, _, new)) in changes.iter().enumerate() {
        if let Err(e) = std::fs::write(path, new) {
            for (written, old, _) in &changes[..=i] {
                let _ = std::fs::write(written, old);
            }
            return Err(OpError::new(
                OpErrorKind::of_io(&e),
                UNREADABLE,
                format!("failed to write {}: {e}", path.display()),
            ));
        }
    }
    Ok(changes
        .into_iter()
        .filter(|(_, old, new)| old != new)
        .map(|(path, _, _)| path)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    fn edit(line: usize, start_col: usize, end_col: usize, new_text: &str) -> RenameEdit {
        RenameEdit {
            file: "a.spec".to_string(),
            line,
            start_col,
            end_col,
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn several_edits_on_one_line_keep_their_columns() {
        let text = "uses [ab, ab]\nab\n";
        // Given left to right; applied right to left.
        let edits = [edit(1, 6, 8, "alpha"), edit(1, 10, 12, "alpha")];
        assert_eq!(apply_edits(text, &edits), "uses [alpha, alpha]\nab\n");
    }

    #[test]
    fn byte_columns_after_multibyte_text_land_on_the_identifier() {
        // "é" and "→" are 2 and 3 bytes: the columns are byte offsets.
        let text = "title \"é→\" ab\n";
        let start = text.find("ab").unwrap();
        let edits = [edit(1, start, start + 2, "beta")];
        assert_eq!(apply_edits(text, &edits), "title \"é→\" beta\n");
    }

    #[test]
    fn edits_on_other_lines_leave_the_rest_and_a_missing_newline_alone() {
        let text = "ab\nkeep\nab";
        let edits = [edit(3, 0, 2, "gamma"), edit(9, 0, 2, "never")];
        assert_eq!(apply_edits(text, &edits), "ab\nkeep\ngamma");
    }

    #[test]
    fn an_entity_id_follows_the_grammar_and_e014() {
        for ok in ["ab", "_x", "auth_token", "A1", &"a".repeat(60)] {
            assert!(validate_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "a",
            "1abc",
            "has-dash",
            "has space",
            "émile",
            &"a".repeat(61),
        ] {
            assert_eq!(validate_id(bad).unwrap_err().code, INVALID_ID, "{bad}");
        }
    }

    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, specforge_project::CompiledProject) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("specforge.json"),
            r#"{"name":"r","extensions":["@specforge/software"]}"#,
        )
        .unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).unwrap();
        }
        let runtime = specforge_component::ComponentRuntime::with_user_cache();
        let project = specforge_project::CompiledProject::compile(
            &specforge_common::ProjectRoot::of(dir.path()).unwrap(),
            Some(std::sync::Arc::new(runtime)),
        );
        (dir, project)
    }

    /// A navigator over `project`, reading its files through `read`.
    fn nav<'p, F: Fn(&str) -> Option<String>>(
        project: &'p specforge_project::CompiledProject,
        read: F,
    ) -> Navigator<'p, F> {
        Navigator::new(crate::view::ProjectView::of(project), read)
    }

    const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
    const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

    #[test]
    fn a_plan_renames_the_declaration_and_every_reference() {
        let (dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
        let read = |f: &str| std::fs::read_to_string(dir.path().join(f)).ok();

        let plan = plan(&nav(&project, read), "session_limit", "session_cap").unwrap();
        assert_eq!(
            plan.affected_files().into_iter().collect::<Vec<_>>(),
            ["limit.spec", "login.spec"]
        );
        apply(&plan, dir.path()).unwrap();
        let login = std::fs::read_to_string(dir.path().join("login.spec")).unwrap();
        assert!(login.contains("invariants [session_cap]"), "{login}");
        let limit = std::fs::read_to_string(dir.path().join("limit.spec")).unwrap();
        assert!(limit.starts_with("invariant session_cap "), "{limit}");
    }

    #[specforge_test(
        behavior = "rename_entity_id",
        verify = "rename leaves strings, comments and verify texts alone"
    )]
    fn a_plan_leaves_prose_alone() {
        let text = "invariant session_limit \"session_limit cap\" {\n  guarantee \"session_limit is never exceeded\"\n}\n\
                    behavior login \"Login\" {\n  invariants [session_limit]\n  // keeps session_limit\n  verify unit \"login respects session_limit\"\n}\n";
        let (dir, project) = project(&[("a.spec", text)]);
        let read = |f: &str| std::fs::read_to_string(dir.path().join(f)).ok();

        let plan = plan(&nav(&project, read), "session_limit", "session_cap").unwrap();
        let at: Vec<(usize, usize)> = plan.edits.iter().map(|e| (e.line, e.start_col)).collect();
        assert_eq!(at, [(1, 10), (5, 14)], "the declaration and the reference");
        apply(&plan, dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.spec")).unwrap(),
            text.replace("invariant session_limit", "invariant session_cap")
                .replace("[session_limit]", "[session_cap]")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_write_puts_every_file_back() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
        let read = |f: &str| std::fs::read_to_string(dir.path().join(f)).ok();
        let plan = plan(&nav(&project, read), "session_limit", "session_cap").unwrap();
        // limit.spec is written first; login.spec cannot be.
        let login = dir.path().join("login.spec");
        std::fs::set_permissions(&login, std::fs::Permissions::from_mode(0o444)).unwrap();
        if std::fs::OpenOptions::new().write(true).open(&login).is_ok() {
            return; // permissions don't bind this user (root)
        }

        let error = apply(&plan, dir.path()).unwrap_err();

        assert_eq!(error.code, UNREADABLE);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("limit.spec")).unwrap(),
            LIMIT
        );
        assert_eq!(std::fs::read_to_string(&login).unwrap(), LOGIN);
    }

    #[test]
    fn a_plan_refuses_what_it_cannot_do_whole() {
        let (dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
        let read = |f: &str| std::fs::read_to_string(dir.path().join(f)).ok();
        let code = |r: Result<RenamePlan, OpError>| r.unwrap_err().code;
        let navigator = nav(&project, read);

        assert_eq!(code(plan(&navigator, "session_limit", "x")), INVALID_ID);
        assert_eq!(code(plan(&navigator, "nope", "fine_name")), "E003");
        assert_eq!(
            plan(&navigator, "nope", "fine_name").unwrap_err().kind,
            OpErrorKind::EntityNotFound
        );
        assert_eq!(code(plan(&navigator, "session_limit", "login")), TAKEN);
        let without_limit = |f: &str| {
            (f != "limit.spec").then(|| std::fs::read_to_string(dir.path().join(f)).unwrap())
        };
        let refused = plan(
            &nav(&project, without_limit),
            "session_limit",
            "session_cap",
        )
        .unwrap_err();
        assert_eq!(refused.code, UNREADABLE);
        assert!(
            refused.message.contains("limit.spec"),
            "{}",
            refused.message
        );
        // A file whose text no longer spells the ID where the graph says
        // (it changed since the compile) is refused the same way.
        let stale = |f: &str| {
            let text = std::fs::read_to_string(dir.path().join(f)).unwrap();
            Some(if f == "login.spec" {
                format!("\n{text}")
            } else {
                text
            })
        };
        let refused = plan(&nav(&project, stale), "session_limit", "session_cap").unwrap_err();
        assert_eq!(refused.code, UNREADABLE);
        assert!(
            refused.message.contains("login.spec"),
            "{}",
            refused.message
        );
    }
}
