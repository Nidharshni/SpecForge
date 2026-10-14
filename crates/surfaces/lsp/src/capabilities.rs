//! What `initialize` answers (`lsp_initialize`): the server's identity and what it serves. It
//! is one static value: the semantic token legend lists every standard token type and is sent
//! before any extension loads, so no capability depends on the project.

use tower_lsp::lsp_types::*;

/// What asks for a completion: a space (a keyword, a field's value) and `[` (a reference list).
/// Nothing completes inside a string, so `"` triggers nothing (ADR 0023 D6).
const COMPLETION_TRIGGERS: [&str; 2] = [" ", "["];

/// What `initialize` answers.
pub fn initialize_result() -> InitializeResult {
    InitializeResult {
        server_info: Some(ServerInfo {
            name: "specforge-lsp".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(COMPLETION_TRIGGERS.map(String::from).to_vec()),
                ..Default::default()
            }),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    legend: SemanticTokensLegend {
                        token_types: crate::TOKEN_TYPES
                            .iter()
                            .map(|t| SemanticTokenType::new(t))
                            .collect(),
                        token_modifiers: crate::TOKEN_MODIFIERS
                            .iter()
                            .map(|m| SemanticTokenModifier::new(m))
                            .collect(),
                    },
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    range: None,
                    ..Default::default()
                }),
            ),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_range_formatting_provider: Some(OneOf::Left(true)),
            ..Default::default()
        },
    }
}
