use super::*;
use crate::protocol::error_codes;
use specforge_common::Severity;
use specforge_graph::Graph;
use specforge_ops::{OpError, OpErrorKind};

const ALL: [ErrorCode; 12] = [
    ErrorCode::InvalidInput,
    ErrorCode::CompilationFailed,
    ErrorCode::EntityNotFound,
    ErrorCode::FileNotFound,
    ErrorCode::ExtensionNotFound,
    ErrorCode::PermissionDenied,
    ErrorCode::Timeout,
    ErrorCode::NotInitialized,
    ErrorCode::SchemaMismatch,
    ErrorCode::InternalError,
    ErrorCode::Conflict,
    ErrorCode::PreconditionFailed,
];

#[specforge_test_macros::test(
    behavior = "state_mcp_messages_as_spec_types",
    verify = "an McpError is serialized from its type, the isError content and a JSON-RPC error's data alike"
)]
fn an_error_serializes_as_its_type() {
    let mut error = McpError::new(ErrorCode::InvalidInput, "no")
        .with_argument("scope")
        .with_diagnostic(&Diagnostic::untyped("E003", Severity::Error, "gone"));
    error.prompt = Some("specforge://prompts/context".into());
    // As a JSON value (its keys sorted, as every JSON-RPC document the
    // server builds from a value).
    let text = serde_json::to_value(&error).unwrap().to_string();

    // The isError result's text is the type, byte for byte.
    let reply = envelope(
        ToolOutcome::Refused(Box::new(error.clone())),
        None,
        false,
        false,
    );
    let content = reply.result.as_ref().unwrap()["content"][0]["text"].clone();
    assert_eq!(content.as_str(), Some(text.as_str()));

    // And so is a JSON-RPC error's data.
    let rpc = error.into_rpc_error();
    assert_eq!(rpc.data.unwrap().to_string(), text);
}

#[test]
fn every_operation_kind_has_an_error_code() {
    for (kind, code) in [
        (OpErrorKind::InvalidInput, ErrorCode::InvalidInput),
        (OpErrorKind::EntityNotFound, ErrorCode::EntityNotFound),
        (OpErrorKind::FileNotFound, ErrorCode::FileNotFound),
        (OpErrorKind::ExtensionNotFound, ErrorCode::ExtensionNotFound),
        (OpErrorKind::Conflict, ErrorCode::Conflict),
        (OpErrorKind::SchemaMismatch, ErrorCode::SchemaMismatch),
        (
            OpErrorKind::PreconditionFailed,
            ErrorCode::PreconditionFailed,
        ),
        (OpErrorKind::PermissionDenied, ErrorCode::PermissionDenied),
        (OpErrorKind::Timeout, ErrorCode::Timeout),
        (OpErrorKind::Internal, ErrorCode::InternalError),
    ] {
        assert_eq!(ErrorCode::from(kind), code, "{kind:?}");
        // One vocabulary: the name an operation reports a kind under is
        // the name MCP sends for it.
        assert_eq!(code.as_str(), kind.as_str(), "{kind:?}");
    }
}

#[test]
fn an_operation_failure_names_its_file() {
    let error: McpError = OpError::new(
        OpErrorKind::FileNotFound,
        "file_not_found",
        "'a.spec' is none of this project's sources",
    )
    .with_file("a.spec")
    .with_suggestion("did you mean 'b.spec'?")
    .into();
    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["file"], "a.spec");
    assert_eq!(json["data"]["suggestion"], "did you mean 'b.spec'?");
    assert!(json.get("entity_id").is_none(), "{json}");
}

#[test]
fn an_operation_failure_is_an_mcp_error_by_its_kind() {
    let error: McpError = OpError::new(OpErrorKind::Conflict, "entity_exists", "taken")
        .with_entity("alpha")
        .with_suggestion("pick another")
        .into();
    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["code"], "conflict");
    assert_eq!(json["entity_id"], "alpha");
    assert_eq!(json["data"]["suggestion"], "pick another");
    assert!(json.get("diagnostic").is_none(), "{json}");

    let error: McpError = OpError::coded(
        specforge_ops::OpErrorKind::InvalidInput,
        specforge_common::codes::E062,
        "the budget is too small",
    )
    .with_suggestion("raise it")
    .into();
    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["code"], "invalid_input");
    assert_eq!(json["diagnostic"]["code"], "E062");
    assert_eq!(json["diagnostic"]["suggestion"], "raise it");
    assert_eq!(json["message"], "the budget is too small");
}

#[test]
fn every_error_code_has_a_json_rpc_code() {
    for code in ALL {
        let rpc = code.rpc_code();
        let expected = match code.as_str() {
            "invalid_input" | "entity_not_found" | "extension_not_found" | "conflict" => {
                error_codes::INVALID_PARAMS
            }
            "not_initialized" => error_codes::INVALID_REQUEST,
            _ => error_codes::INTERNAL_ERROR,
        };
        assert_eq!(rpc, expected, "{}", code.as_str());
    }
}

#[test]
fn a_prompts_refusal_names_the_prompt() {
    let mut error = McpError::new(ErrorCode::InvalidInput, "no");
    assert!(
        serde_json::to_value(&error)
            .unwrap()
            .get("prompt")
            .is_none()
    );
    error.prompt = Some("specforge://prompts/context".into());
    assert_eq!(
        serde_json::to_value(&error).unwrap()["prompt"],
        "specforge://prompts/context"
    );
    assert!(serde_json::to_value(&error).unwrap().get("tool").is_none());
}

#[test]
fn a_resources_refusal_names_the_uri() {
    let mut error = McpError::new(ErrorCode::InvalidInput, "no");
    assert!(serde_json::to_value(&error).unwrap().get("uri").is_none());
    error.uri = Some("specforge://graph?depth=two".into());
    assert_eq!(
        serde_json::to_value(&error).unwrap()["uri"],
        "specforge://graph?depth=two"
    );
    assert!(
        serde_json::to_value(&error)
            .unwrap()
            .get("resource")
            .is_none()
    );
    assert!(
        serde_json::to_value(&error)
            .unwrap()
            .get("prompt")
            .is_none()
    );
}

#[test]
fn a_refusal_without_is_error_is_invalid_params_when_the_client_can_fix_it() {
    let rpc = |error: McpError| error.into_rpc_error();
    // An argument it named, whatever its code.
    let named = McpError::new(ErrorCode::FileNotFound, "path not found").with_argument("path");
    let named = rpc(named);
    assert_eq!(named.code, error_codes::INVALID_PARAMS);
    assert_eq!(named.data.as_ref().unwrap()["argument"], "path");
    // A code that is input.
    assert_eq!(
        rpc(McpError::new(ErrorCode::EntityNotFound, "no")).code,
        error_codes::INVALID_PARAMS
    );
    // A failure on the server's side, and a missing project (no params
    // fix it).
    assert_eq!(
        rpc(McpError::new(ErrorCode::InternalError, "no")).code,
        error_codes::INTERNAL_ERROR
    );
    assert_eq!(
        rpc(McpError::new(ErrorCode::PreconditionFailed, "no")).code,
        error_codes::INTERNAL_ERROR
    );
}

#[test]
fn an_unknown_entity_carries_its_e003() {
    let json = serde_json::to_value(entity_not_found(&Graph::new(), "ghost")).unwrap();
    assert_eq!(json["code"], "entity_not_found");
    assert_eq!(json["entity_id"], "ghost");
    assert_eq!(json["diagnostic"]["code"], "E003");
    assert_eq!(
        json["message"],
        "unresolved entity 'ghost' — not found in graph"
    );
}
