//! What a diagnostics publish sends, computed from the LSP state alone
//! (ADR 0023): placement, conversion and the files to clear, before any
//! client call.

use std::collections::{BTreeMap, BTreeSet};

use specforge_common::SourceSpan;
use tower_lsp::lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    NumberOrString, Range, Url,
};

use crate::LspState;
use crate::navigation::Compiled;

/// What one publish sends, read from the state alone (so it is tested
/// without a client): each target file's diagnostics, placed and
/// converted, with the version of the buffer they were computed from. An
/// empty list clears the file.
#[derive(Debug, Clone, Default)]
pub struct Publication {
    /// Each target file's part, by session key.
    pub files: BTreeMap<String, FilePublication>,
    /// The key a diagnostic about no entity went on: the next publish's anchor.
    pub anchor: Option<String>,
}

/// One file's part of a [`Publication`].
#[derive(Debug, Clone, Default)]
pub struct FilePublication {
    /// As the client receives them.
    pub diagnostics: Vec<Diagnostic>,
    /// As code actions read them back.
    pub placed: Vec<specforge_common::Diagnostic>,
    /// The editor's version of the text these diagnostics are positions in
    /// (the buffer the project was compiled from); `None` for a file
    /// compiled from disk.
    pub version: Option<i32>,
}

impl Publication {
    /// What the project reports now: a diagnostic on the file its span
    /// names (the compiled project locates one about entities, ADR 0227),
    /// related information at each other entity's name; one about none on
    /// `edited`, else on the anchor while it is open, else on the first
    /// open document. Targets: every file with diagnostics, every file
    /// published before, `edited` and `touched`.
    pub fn of(state: &LspState, edited: Option<&str>, touched: &[String]) -> Publication {
        let files = state.files();
        let nowhere = edited
            .or_else(|| files.anchor())
            .or_else(|| files.open_keys().next())
            .map(str::to_string);
        let diagnostics = state
            .session()
            .map(|s| s.project().diagnostics())
            .unwrap_or_default();
        let compiled = Compiled::new(state);
        let nav = compiled.navigator();
        let mut by_file: BTreeMap<String, FilePublication> = BTreeMap::new();
        let mut anchor = None;
        for diagnostic in &diagnostics {
            let key = match &diagnostic.span {
                Some(span) => span.file.as_str().to_string(),
                None => match &nowhere {
                    Some(key) => {
                        anchor = Some(key.clone());
                        key.clone()
                    }
                    None => continue,
                },
            };
            let mut lsp = diagnostic_to_lsp(diagnostic, |span| compiled.compiled_range(span));
            let related = also_about(&compiled, &nav, diagnostic);
            if !related.is_empty() {
                lsp.related_information = Some(related);
            }
            let file = by_file.entry(key).or_default();
            file.diagnostics.push(lsp);
            file.placed.push(diagnostic.clone());
        }
        let mut targets: BTreeSet<String> = touched.iter().cloned().collect();
        targets.extend(files.published_keys().map(str::to_string));
        targets.extend(edited.map(str::to_string));
        for key in targets {
            by_file.entry(key).or_default();
        }
        for (key, file) in &mut by_file {
            file.version = state.compiled_version(key);
        }
        Publication {
            files: by_file,
            anchor,
        }
    }
}

/// Related information at the name of each entity a diagnostic's data names but the first,
/// which the compiled project located it at (ADR 0016 D7, ADR 0227).
fn also_about(
    compiled: &Compiled,
    nav: &specforge_ops::read::navigate::Navigator<'_>,
    diagnostic: &specforge_common::Diagnostic,
) -> Vec<DiagnosticRelatedInformation> {
    if diagnostic.data.is_none() {
        return Vec::new();
    }
    let subjects = specforge_ops::read::navigate::subjects(compiled.state().graph(), diagnostic);
    subjects
        .iter()
        .skip(1)
        .filter_map(|node| {
            let name = nav
                .definition(node.id.raw.as_str())
                .map(|d| d.name)
                .unwrap_or_else(|_| node.source_span.clone());
            Some(DiagnosticRelatedInformation {
                location: compiled.location(&name)?,
                message: format!("also about '{}'", node.id.raw),
            })
        })
        .collect()
}

/// A diagnostic as the client receives it; `range_of` converts its span.
/// One whose span cannot be converted (its file's text is unknown) stays on
/// its file at the start, as one with no span does: never at columns read
/// from another text.
pub(crate) fn diagnostic_to_lsp(
    diag: &specforge_common::Diagnostic,
    range_of: impl Fn(&SourceSpan) -> Option<Range>,
) -> Diagnostic {
    let range = diag.span.as_ref().and_then(range_of).unwrap_or_default();
    Diagnostic {
        range,
        code: Some(NumberOrString::String(diag.code.clone())),
        // C4-10: editors can render this as a "view docs" link to the
        // code's section of docs/diagnostics.md; none for a code the catalog does not describe
        // (a third-party code, or an extension's use of a code another owner has).
        code_description: specforge_diagnostics::describes(&diag.code, diag.origin())
            .and_then(|entry| Url::parse(&entry.docs_href()).ok())
            .map(|href| CodeDescription { href }),
        severity: Some(match diag.severity {
            specforge_common::Severity::Error => DiagnosticSeverity::ERROR,
            specforge_common::Severity::Warning => DiagnosticSeverity::WARNING,
            specforge_common::Severity::Info => DiagnosticSeverity::INFORMATION,
        }),
        source: Some("specforge".into()),
        // C4-08: the suggestion is the actionable half of the diagnostic
        // ("did you mean X / do Y") — surface it in the editor instead of
        // dropping it at the LSP boundary.
        message: match &diag.suggestion {
            Some(suggestion) => format!("{}\n\nsuggestion: {suggestion}", diag.message),
            None => diag.message.clone(),
        },
        // A define block (W143, ADR 0005) is inert code: editors fade it
        // (as for inactive code) instead of only underlining it.
        tags: diag
            .is(specforge_common::codes::W143)
            .then(|| vec![DiagnosticTag::UNNECESSARY]),
        // The typed payload, as the diagnostics JSON presents it: a client
        // echoes it back in a code-action request's context.
        data: diag
            .data
            .as_deref()
            .and_then(|data| serde_json::to_value(data).ok()),
        ..Default::default()
    }
}

#[cfg(test)]
mod docs_link_tests {
    use super::*;

    fn href(code: &str) -> Option<String> {
        let diag = specforge_common::Diagnostic::untyped(
            code,
            specforge_common::Severity::Error,
            "message",
        );
        diagnostic_to_lsp(&diag, |_| Some(Range::default()))
            .code_description
            .map(|d| d.href.to_string())
    }

    /// C6: the "view docs" link points at the code's anchor in
    /// docs/diagnostics.md on the canonical repository (ADR 0004 D6-b), and
    /// only for codes that have an anchor there.
    #[test]
    fn docs_links_only_codes_with_an_anchor_on_the_canonical_repository() {
        assert_eq!(
            href("E001").as_deref(),
            Some("https://github.com/leaderiop/SpecForge/blob/main/docs/diagnostics.md#e001")
        );
        assert!(
            href("R-RES-005").is_some_and(|h| h.ends_with("#r-res-005")),
            "catalogued registry codes are linked"
        );
        assert_eq!(href("E901"), None, "third-party codes have no anchor");
        assert_eq!(
            href("F011"),
            None,
            "codes outside the catalog have no anchor"
        );
        assert_eq!(
            href("E047"),
            None,
            "retired codes have no anchor of their own"
        );
    }
}
