/// JSON-RPC 2.0 standard error codes.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
/// MCP's "Resource not found" in the handshake revisions (2025-03-26 to
/// 2025-11-25, server/resources "Error Handling"); the stateless revision
/// 2026-07-28 answers -32602 instead.
pub const RESOURCE_NOT_FOUND: i64 = -32002;
