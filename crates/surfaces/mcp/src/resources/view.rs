//! The graph views the core resources serve (ADR 0024 D4): `graph`,
//! `context`, `brief` (whole or scoped) and `graph/{entity_id}` are
//! `specforge export` over the call's project view, through the same
//! function. The resource modules read their query through this one.
//! Nothing here reads the server's state.

use specforge_ops::read::export::{ExportField, Format, Request};

use crate::resource::{ReadOutcome, ResourceText};
use crate::tool::{ErrorCode, McpError};
use specforge_ops::view::ProjectView;

/// The query keys a graph view reads.
const KEYS: &str = "scope (or root), depth, kinds and max_tokens";

/// What a graph-view resource URI asks `ops::read::export` for: the template's
/// placeholder (or `?scope=`, alias `?root=`, on an untemplated URI) as the
/// scope, `depth`, `kinds` (comma-separated) and `max_tokens`. Keys and
/// values are percent-decoded; nothing in a query is ignored: an unknown or
/// repeated key, a value that does not parse, `scope` with `root`, and a
/// scope on a templated URI are refused naming the key (ADR 0024 D8).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ViewQuery {
    pub scope: Option<String>,
    /// The key that sent the scope: the template's `entity_id`, or `scope` or `root` as the URI
    /// spelled it.
    pub scope_key: &'static str,
    pub depth: Option<usize>,
    pub kinds: Vec<String>,
    pub max_tokens: Option<usize>,
}

impl ViewQuery {
    /// The query of `uri`; `template` is the resource's template
    /// (`"specforge://context/{entity_id}"`), whose placeholder's value is the
    /// scope.
    pub fn parse(uri: &str, template: Option<&str>) -> Result<Self, Box<McpError>> {
        let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
        let mut parsed = ViewQuery::default();
        if let Some(template) = template {
            let placeholder = path.strip_prefix(prefix(template)).unwrap_or_default();
            parsed.scope = Some(decode("entity_id", placeholder)?);
            parsed.scope_key = "entity_id";
        }
        let mut seen: Vec<String> = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
            let key = decode(raw_key, raw_key)?;
            if seen.contains(&key) {
                return Err(refuse(
                    &key,
                    format!("the query key '{key}' is given twice"),
                ));
            }
            if pair.split_once('=').is_none() {
                return Err(refuse(&key, format!("the query key '{key}' has no value")));
            }
            let value = decode(&key, raw_value)?;
            match key.as_str() {
                "scope" | "root" => {
                    if template.is_some() {
                        return Err(refuse(
                            &key,
                            format!(
                                "the scope of this resource is its entity_id: '{key}' is not read"
                            ),
                        ));
                    }
                    if seen.iter().any(|seen| seen == "scope" || seen == "root") {
                        return Err(refuse(
                            &key,
                            format!(
                                "'scope' and 'root' name the same scope: give one, not '{key}' too"
                            ),
                        ));
                    }
                    if value.is_empty() {
                        return Err(refuse(&key, format!("'{key}' must name an entity")));
                    }
                    parsed.scope = Some(value);
                    parsed.scope_key = if key == "scope" { "scope" } else { "root" };
                }
                // A count is read as a tool's count argument is (ADR 0033 D2:
                // a non-negative integer, or a string holding one), with its
                // wording.
                "depth" | "max_tokens" => {
                    let count =
                        <usize as crate::args::Arg>::read(&key, &serde_json::Value::String(value))
                            .map_err(|message| refuse(&key, message))?;
                    if key == "depth" {
                        parsed.depth = Some(count);
                    } else {
                        parsed.max_tokens = Some(count);
                    }
                }
                "kinds" => {
                    let mut kinds = Vec::new();
                    for kind in value.split(',').map(str::trim) {
                        if kind.is_empty() {
                            return Err(refuse(
                                &key,
                                "'kinds' is a comma-separated list of kinds, none of them empty"
                                    .to_string(),
                            ));
                        }
                        kinds.push(kind.to_string());
                    }
                    parsed.kinds = kinds;
                }
                other => {
                    return Err(refuse(
                        other,
                        format!("unknown query key '{other}': this resource reads {KEYS}"),
                    ));
                }
            }
            seen.push(key);
        }
        Ok(parsed)
    }

    /// The query key the export's refusal about `field` is about: the scope's own key, or the key
    /// a limit is read from; none for the schema version, which no resource sends.
    pub fn argument(&self, field: ExportField) -> Option<&'static str> {
        match field {
            ExportField::Scope => Some(self.scope_key),
            ExportField::Depth => Some("depth"),
            ExportField::MaxTokens => Some("max_tokens"),
            ExportField::SchemaVersion | ExportField::Schema => None,
        }
    }

    /// The export request for `format` under the export schema policy
    /// (`Schema::Default`).
    pub fn request(&self, format: Format) -> Request<'_> {
        Request {
            format: Some(format),
            scope: self.scope.as_deref(),
            depth: self.depth,
            kinds: self.kinds.iter().map(String::as_str).collect(),
            max_tokens: self.max_tokens,
            ..Request::default()
        }
    }
}

/// The text of `template` before its first `{placeholder}`.
pub(super) fn prefix(template: &str) -> &str {
    template.split_once('{').map_or(template, |(head, _)| head)
}

/// A query the resource cannot read: `invalid_input` naming `key`.
fn refuse(key: &str, message: String) -> Box<McpError> {
    Box::new(McpError::new(ErrorCode::InvalidInput, message).with_argument(key))
}

/// `text` with its percent-escapes decoded (RFC 3986 §2.1); `what` names it
/// when the escapes are malformed or decode to no UTF-8.
pub(super) fn decode(what: &str, text: &str) -> Result<String, Box<McpError>> {
    let malformed = || refuse(what, format!("'{text}' is not valid percent-encoded UTF-8"));
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3).ok_or_else(malformed)?;
            decoded.push(u8::from_str_radix(hex, 16).map_err(|_| malformed())?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| malformed())
}

/// A resource that takes no query: any is refused naming its first key.
pub(super) fn no_query(uri: &str) -> Result<(), Box<McpError>> {
    match uri.split_once('?') {
        Some((_, query)) if !query.is_empty() => {
            let key = query.split(['&', '=']).next().unwrap_or_default();
            Err(refuse(
                key,
                format!("this resource reads no query: '{key}' is not read"),
            ))
        }
        _ => Ok(()),
    }
}

/// A graph view (`graph`, `context`, `brief`, with or without a scope): the
/// text `specforge export` writes for the same request, through the same
/// function over the call's project view (ADR 0004 D3-a). A failure is the
/// operation's McpError.
pub(crate) fn export_view(
    view: ProjectView<'_>,
    uri: &str,
    format: Format,
    template: Option<&str>,
) -> ReadOutcome {
    let query = ViewQuery::parse(uri, template)?;
    if template.is_some() {
        check_entity_id(query.scope.as_deref().unwrap_or_default())?;
    }
    exported(view, &query, format)
}

/// An entity ID the URI template names: told apart, when it cannot be one
/// (the 400 case), from a well-formed one that names no entity (the 404).
pub(super) fn check_entity_id(entity_id: &str) -> Result<(), Box<McpError>> {
    if entity_id.is_empty() {
        return Err(refuse(
            "entity_id",
            "Malformed entity ID: must not be empty".to_string(),
        ));
    }
    if !entity_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
    {
        return Err(refuse(
            "entity_id",
            format!(
                "Malformed entity ID: {entity_id:?} may only contain letters, digits, '_', '.', ':' and '-'"
            ),
        ));
    }
    Ok(())
}

pub(super) fn exported(view: ProjectView<'_>, query: &ViewQuery, format: Format) -> ReadOutcome {
    specforge_ops::read::export::export(&view, &query.request(format))
        .map(ResourceText::json)
        .map_err(|refusal| Box::new(McpError::refused(refusal, |field| query.argument(field))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "read_mcp_arguments_as_declared",
        verify = "a prompt's or a resource read's refusal names the argument or query key it is about"
    )]
    fn a_graph_view_names_the_query_key_its_refusal_is_about() {
        let key = |uri: &str, template: Option<&str>| {
            let query = ViewQuery::parse(uri, template).unwrap();
            query.argument(ExportField::Scope)
        };
        assert_eq!(key("specforge://graph?root=ghost", None), Some("root"));
        assert_eq!(key("specforge://graph?scope=ghost", None), Some("scope"));
        assert_eq!(
            key(
                "specforge://graph/ghost",
                Some("specforge://graph/{entity_id}")
            ),
            Some("entity_id")
        );
        let query = ViewQuery::parse("specforge://graph?max_tokens=5", None).unwrap();
        assert_eq!(query.argument(ExportField::MaxTokens), Some("max_tokens"));
        assert_eq!(query.argument(ExportField::Depth), Some("depth"));
        assert_eq!(query.argument(ExportField::SchemaVersion), None);
    }
}
