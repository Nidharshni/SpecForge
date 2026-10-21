//! Renaming an entity ID: the MCP `specforge.rename` tool and the LSP's
//! `textDocument/rename` plan the same edits under the same rules. MCP
//! runs it ([`run`]: planned, then written as one project change unless the
//! request previews) and recompiles; the LSP hands the plan's edits to the
//! editor. The edits are exactly the tokens the graph's finder reads (ADR 0163: the
//! declaration's name and each reference's token): text in strings,
//! comments and verify statements that mentions the ID is not a reference,
//! and is left alone (ADR 0016).

use crate::change::ProjectChange;
use crate::read::navigate::{Navigator, Precision, TextEdit, apply_edits};
use crate::view::EntityField;
use crate::{Caller, OpError, OpErrorKind, Refusal, Wrote};
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_parser::lex::{is_identifier, lex};
use specforge_registry::ID_LENGTH;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

/// The field of a rename's request a refusal can be about (ADR 0072).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameField {
    /// The entity to rename.
    OldId,
    /// The ID to rename it to.
    NewId,
}

/// The new ID is not a legal entity ID.
pub const INVALID_ID: &str = "invalid_name";
/// The new ID already names an entity.
pub const TAKEN: &str = "entity_exists";
/// The entity's name is not an entity ID token (a spec block's name is a string): nothing to rename.
pub const NOT_RENAMEABLE: &str = "not_renameable";
/// A source that does not parse spells the ID: the rename cannot see its references.
pub const UNPARSED: &str = "source_unparsed";
/// Whether `id` is a legal entity ID: the grammar's identifier
/// ([`is_identifier`]) within E014's length ([`ID_LENGTH`]), and no word
/// `kinds` reserves (E013).
pub fn validate_id(id: &str, kinds: &specforge_registry::KindRegistry) -> Result<(), OpError> {
    if !(is_identifier(id) && ID_LENGTH.contains(&id.len())) {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            INVALID_ID,
            format!(
                "invalid entity ID '{id}': it must be 2-60 characters, letters, digits and \
                 underscores, starting with a letter or underscore"
            ),
        ));
    }
    if kinds.reserves(id) {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            INVALID_ID,
            format!(
                "invalid entity ID '{id}': it is a reserved word (E013), a structural keyword or the keyword \
                 of a declared kind"
            ),
        ));
    }
    Ok(())
}

/// The edits that rename one entity: its declaration and every reference.
#[derive(Debug, Clone)]
pub struct RenamePlan {
    pub old_id: String,
    pub new_id: String,
    /// One per occurrence: the token's span as navigation found it, relative to the spec root; its
    /// text becomes the new ID.
    pub edits: Vec<TextEdit>,
    /// Each affected file's text, as the plan read it: the text the edits are positions in.
    planned: BTreeMap<String, Arc<str>>,
    /// Where the edits' files are keyed from: the spec root of the view the plan read.
    spec_root: PathBuf,
}

impl RenamePlan {
    /// The files the edits touch, sorted.
    pub fn affected_files(&self) -> BTreeSet<&str> {
        self.edits.iter().map(|e| e.span.file.as_str()).collect()
    }
}

/// Plan renaming `old_id` to `new_id`: one edit per occurrence `nav`
/// finds, the declaration's name and each incoming reference's token,
/// each file read through the navigator. A rename is all or nothing: one
/// that cannot see every token (a file it cannot read, or whose text no
/// longer spells the ID where the graph says, or a source that does not parse
/// and spells the ID as a name) is refused, rather than guessing or leaving a
/// reference behind; so is a spec block, whose name is a string, not an
/// entity ID (`not_renameable`). Refused with `config_invalid`
/// when the compile's `specforge.json` does not say which files are the
/// project's sources
/// ([`ConfigProblem::blocks_rewrites`](specforge_common::ConfigProblem::blocks_rewrites)).
pub fn plan(
    nav: &Navigator<'_>,
    old_id: &str,
    new_id: &str,
) -> Result<RenamePlan, Refusal<RenameField>> {
    // A rename rewrites the project's sources: a config that does not say
    // which files they are refuses it, from the compile's problems (as
    // `remove` refuses from them, ADR 0015 M5), before anything is planned.
    crate::config::refuse_rewrite(nav.view().env().config_problems())?;
    validate_id(new_id, nav.view().registries().kinds())
        .map_err(|error| error.about(RenameField::NewId))?;
    let graph = nav.view().graph();
    let node = nav.view().entity(old_id, RenameField::OldId)?.node;
    if graph.node(new_id).is_some() {
        return Err(OpError::new(
            OpErrorKind::Conflict,
            TAKEN,
            format!("cannot rename '{old_id}': '{new_id}' exists"),
        )
        .with_entity(old_id)
        .about(RenameField::NewId));
    }
    if node.kind.raw.as_str() == specforge_common::structural::SPEC {
        return Err(OpError::new(
            OpErrorKind::InvalidInput,
            NOT_RENAMEABLE,
            format!(
                "cannot rename '{old_id}': a spec block's name is a string, not an entity ID; edit the string in {}",
                node.source_span.file
            ),
        )
        .with_entity(old_id)
        .about(RenameField::OldId));
    }
    let occurrences = nav
        .tokens(old_id, true)
        .map_err(|refusal| refusal.map_subject(|EntityField::EntityId| Some(RenameField::OldId)))?;
    let unreadable: BTreeSet<&str> = occurrences
        .iter()
        .filter(|o| o.precision == Precision::Entity)
        .map(|o| o.span.file.as_str())
        .collect();
    if !unreadable.is_empty() {
        let files: Vec<&str> = unreadable.into_iter().collect();
        return Err(OpError::new(
            OpErrorKind::Internal,
            crate::FILE_UNREADABLE,
            format!(
                "cannot rename '{old_id}': {} do not spell it where the graph says",
                files.join(", ")
            ),
        )
        .into());
    }
    let unparsed = unparsed(nav, old_id);
    if !unparsed.is_empty() {
        return Err(OpError::new(
            OpErrorKind::PreconditionFailed,
            UNPARSED,
            format!(
                "cannot rename '{old_id}': {} do not parse, and spell it where no reference can be read",
                unparsed.join(", ")
            ),
        )
        .with_suggestion("fix the syntax errors `specforge check` reports (E001), then rename")
        .with_entity(old_id)
        .into());
    }
    let edits: Vec<TextEdit> = occurrences
        .iter()
        .map(|o| TextEdit {
            span: o.span.clone(),
            new_text: new_id.to_string(),
        })
        .collect();
    let planned = occurrences
        .iter()
        .filter_map(|o| {
            let file = o.span.file.as_str();
            Some((file.to_string(), nav.source_text(file)?))
        })
        .collect();
    Ok(RenamePlan {
        old_id: old_id.to_string(),
        new_id: new_id.to_string(),
        edits,
        planned,
        spec_root: nav.view().env().spec_root().to_path_buf(),
    })
}

/// The sources whose parse failed (E001) and whose text spells `id` as a name (an identifier or ref
/// ID lexeme, so not in a string or a comment), in key order: references the graph does not hold,
/// which a rename would leave behind. A source whose text is unknown is not judged.
fn unparsed<'n>(nav: &'n Navigator<'_>, id: &str) -> Vec<&'n str> {
    nav.view()
        .project()
        .parsed()
        .filter(|(_, file)| !file.errors.is_empty())
        .filter(|(key, _)| {
            nav.source_text(key).is_some_and(|text| {
                lex(&text)
                    .iter()
                    .any(|l| l.is_name() && l.text(&text) == id)
            })
        })
        .map(|(key, _)| key)
        .collect()
}

/// Write `plan`'s edits to the files under the spec root it was planned from as one project change (ADR 0128): every file is checked
/// against the text the plan was made on, then replaced; if a write fails, every file already written is put back,
/// and one that cannot be is the error's writes. A failure is the change's own (ADR 0155). Returns the files it wrote.
///
/// Every file is resolved through the caller's containment first (ADR 0104), and read and written where it
/// landed: one that leaves it, by a symlink, refuses the rename (`outside_root`) before any file is
/// written.
pub fn apply(plan: &RenamePlan, caller: Caller<'_>) -> Result<crate::Writes, OpError> {
    let spec_root = plan.spec_root.as_path();
    let mut change = ProjectChange::new(spec_root, format!("cannot rename '{}'", plan.old_id));
    for file in plan.affected_files() {
        let held = caller.containment().resolve(&spec_root.join(file))?;
        // The edits are positions in the planned text: another text refuses the whole rename.
        let Some(planned) = plan.planned.get(file) else {
            return Err(OpError::new(
                OpErrorKind::Internal,
                crate::FILE_UNREADABLE,
                format!(
                    "cannot rename '{}': no text was planned for {file}",
                    plan.old_id
                ),
            ));
        };
        let Some(edited) = apply_edits(planned, plan.edits.iter().filter(|e| e.span.file == file))
        else {
            return Err(OpError::new(
                OpErrorKind::Internal,
                crate::FILE_UNREADABLE,
                format!(
                    "cannot rename '{}': its edits do not lie in the text planned for {file}",
                    plan.old_id
                ),
            ));
        };
        change.stage(held, Some(planned.as_bytes().to_vec()), edited.into_bytes());
    }
    change.commit()
}

/// What a rename did: the plan it made, and whether it wrote (ADR 0162).
#[derive(Debug, Clone)]
pub struct Renamed {
    pub plan: RenamePlan,
    pub wrote: Wrote,
}

/// Plan renaming `old_id` to `new_id` over `nav` ([`plan`]) and write its edits as one project change ([`apply`])
/// unless `dry_run`. Refused as [`plan`] refuses, before anything is written, or with [`apply`]'s error: about no
/// field, its writes the files it could not put back.
pub fn run(
    nav: &Navigator<'_>,
    old_id: &str,
    new_id: &str,
    dry_run: bool,
    caller: Caller<'_>,
) -> Result<Renamed, Refusal<RenameField>> {
    let plan = plan(nav, old_id, new_id)?;
    let wrote = Wrote::unless_preview(dry_run, || apply(&plan, caller))?;
    Ok(Renamed { plan, wrote })
}

/// What `specforge.rename` answers (`McpRenameResult`) but its edits, which are the spec's `TextEdit` and MCP's to
/// spell, flattened beside it (ADR 0164 D3, ADR 0052 D2).
#[derive(Debug, Clone, Serialize, Shape)]
pub struct RenameDocument {
    pub old_name: String,
    pub new_name: String,
    /// The files the edits touch, relative to the spec root, sorted.
    pub affected_files: Vec<String>,
    /// The rename was only planned: nothing was written.
    pub dry_run: bool,
}

impl Renamed {
    /// The document `specforge.rename` answers.
    pub fn document(&self) -> RenameDocument {
        RenameDocument {
            old_name: self.plan.old_id.clone(),
            new_name: self.plan.new_id.clone(),
            affected_files: self
                .plan
                .affected_files()
                .into_iter()
                .map(String::from)
                .collect(),
            dry_run: self.wrote.previewed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_test_macros::test as specforge_test;

    #[test]
    fn an_entity_id_follows_the_grammar_and_e014() {
        let none = specforge_registry::KindRegistry::new();
        for ok in ["ab", "_x", "auth_token", "A1", &"a".repeat(60)] {
            assert!(validate_id(ok, &none).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "a",
            "1abc",
            "has-dash",
            "has space",
            "émile",
            "defaults",
            "spec",
            &"a".repeat(61),
        ] {
            assert_eq!(
                validate_id(bad, &none).unwrap_err().code,
                INVALID_ID,
                "{bad}"
            );
        }
    }

    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, specforge_project::CompiledProject) {
        project_with_config(
            r#"{"name":"r","extensions":["@specforge/software"]}"#,
            files,
        )
    }

    fn project_with_config(
        config: &str,
        files: &[(&str, &str)],
    ) -> (tempfile::TempDir, specforge_project::CompiledProject) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("specforge.json"), config).unwrap();
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

    /// A navigator over `project`, reading the text it was compiled from.
    fn nav(project: &specforge_project::CompiledProject) -> Navigator<'_> {
        Navigator::new(crate::view::ProjectView::of(project))
    }

    /// `project` as if the compile had no text of `limit.spec`.
    fn without_limit(
        project: &specforge_project::CompiledProject,
    ) -> specforge_project::CompiledProject {
        project.with_source_texts(
            project
                .source_texts()
                .into_iter()
                .filter(|(key, _)| key != "limit.spec"),
        )
    }

    const LIMIT: &str = "invariant session_limit \"Limit\" {\n  guarantee \"x\"\n}\n";
    const LOGIN: &str = "behavior login \"Login\" {\n  invariants [session_limit]\n}\n";

    #[specforge_test(
        behavior = "rename_entity_id",
        verify = "a rename is refused when specforge.json does not say which files are the project's sources"
    )]
    fn a_config_that_hides_the_sources_refuses_the_rename() {
        let hiding = r#"{"name":"r","extensions":["@specforge/software"],"spec_root":["spec"]}"#;
        let (_dir, project) = project_with_config(hiding, &[("limit.spec", LIMIT)]);
        let refused = plan(&nav(&project), "session_limit", "session_cap").unwrap_err();
        assert_eq!(refused.error.code, "config_invalid");
        assert!(
            refused.error.message.contains("\"spec_root\""),
            "{}",
            refused.error.message
        );
        // The refusal wins over a bad name.
        let refused = plan(&nav(&project), "session_limit", "1 bad").unwrap_err();
        assert_eq!(refused.error.code, "config_invalid");

        // A problem the plan goes around plans as usual.
        let around = r#"{"name":7,"extensions":["@specforge/software"]}"#;
        let (_dir, project) = project_with_config(around, &[("limit.spec", LIMIT)]);
        assert!(plan(&nav(&project), "session_limit", "session_cap").is_ok());
    }

    #[test]
    fn a_plan_renames_the_declaration_and_every_reference() {
        let (dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);

        let plan = plan(&nav(&project), "session_limit", "session_cap").unwrap();
        assert_eq!(
            plan.affected_files().into_iter().collect::<Vec<_>>(),
            ["limit.spec", "login.spec"]
        );
        apply(&plan, crate::testing::caller()).unwrap();
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

        let plan = plan(&nav(&project), "session_limit", "session_cap").unwrap();
        let at: Vec<(usize, usize)> = plan
            .edits
            .iter()
            .map(|e| (e.span.start_line, e.span.start_col))
            .collect();
        assert_eq!(at, [(1, 11), (5, 15)], "the declaration and the reference");
        apply(&plan, crate::testing::caller()).unwrap();
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
        let plan = plan(&nav(&project), "session_limit", "session_cap").unwrap();
        // limit.spec is written first; login.spec cannot be.
        let login = dir.path().join("login.spec");
        std::fs::set_permissions(&login, std::fs::Permissions::from_mode(0o444)).unwrap();
        if std::fs::OpenOptions::new().write(true).open(&login).is_ok() {
            return; // permissions don't bind this user (root)
        }

        let error = apply(&plan, crate::testing::caller()).unwrap_err();

        assert_eq!(error.code, crate::FILE_UNWRITABLE);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("limit.spec")).unwrap(),
            LIMIT
        );
        assert_eq!(std::fs::read_to_string(&login).unwrap(), LOGIN);
    }

    #[cfg(unix)]
    #[test]
    fn apply_writes_nothing_when_one_file_leaves_the_containment() {
        let (dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
        let plan = plan(&nav(&project), "session_limit", "session_cap").unwrap();
        // login.spec is a symlink to a file outside the directory.
        let outside = tempfile::tempdir().unwrap();
        let elsewhere = outside.path().join("login.spec");
        std::fs::write(&elsewhere, LOGIN).unwrap();
        std::fs::remove_file(dir.path().join("login.spec")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.path().join("login.spec")).unwrap();

        let held = crate::Containment::within(dir.path());
        let error = apply(&plan, crate::testing::bounded(&held)).unwrap_err();

        assert_eq!(error.code, crate::OUTSIDE_ROOT, "{error:?}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("limit.spec")).unwrap(),
            LIMIT
        );
        assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), LOGIN);
    }

    #[test]
    fn a_plan_refuses_what_it_cannot_do_whole() {
        let (_dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
        let code = |r: Result<RenamePlan, Refusal<RenameField>>| r.unwrap_err().error.code;
        let navigator = nav(&project);

        assert_eq!(code(plan(&navigator, "session_limit", "x")), INVALID_ID);
        assert_eq!(code(plan(&navigator, "nope", "fine_name")), "E003");
        assert_eq!(
            plan(&navigator, "nope", "fine_name")
                .unwrap_err()
                .error
                .kind,
            OpErrorKind::EntityNotFound
        );
        assert_eq!(code(plan(&navigator, "session_limit", "login")), TAKEN);
        let without_limit = without_limit(&project);
        let refused = plan(&nav(&without_limit), "session_limit", "session_cap").unwrap_err();
        assert_eq!(refused.error.code, crate::FILE_UNREADABLE);
        assert!(
            refused.error.message.contains("limit.spec"),
            "{}",
            refused.error.message
        );
        // A file whose text no longer spells the ID where the graph says
        // (it changed since the compile) is refused the same way.
        let stale = project.with_source_texts(project.source_texts().into_iter().map(|(k, t)| {
            let text = if k == "login.spec" {
                format!("\n{t}")
            } else {
                t.to_string()
            };
            (k, text)
        }));
        let refused = plan(&nav(&stale), "session_limit", "session_cap").unwrap_err();
        assert_eq!(refused.error.code, crate::FILE_UNREADABLE);
        assert!(
            refused.error.message.contains("login.spec"),
            "{}",
            refused.error.message
        );
    }

    #[specforge_test(
        invariant = "mcp_structured_error_responses",
        verify = "an operation states the request field each refusal is about, and a refusal of the project or the environment is about none"
    )]
    fn rename_states_which_field_a_refusal_is_about() {
        let (_dir, project) = project(&[("limit.spec", LIMIT), ("login.spec", LOGIN)]);
        let navigator = nav(&project);
        let subject = |old: &str, new: &str| plan(&navigator, old, new).unwrap_err().subject;

        assert_eq!(subject("session_limit", "x"), Some(RenameField::NewId));
        assert_eq!(subject("session_limit", "login"), Some(RenameField::NewId));
        assert_eq!(subject("nope", "fine_name"), Some(RenameField::OldId));
        // A file the rename cannot read is a refusal of the environment: about no field.
        let without_limit = without_limit(&project);
        let refused = plan(&nav(&without_limit), "session_limit", "session_cap");
        assert_eq!(refused.unwrap_err().subject, None);
    }
}
