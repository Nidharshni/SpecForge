//! The graph views the core resources serve (ADR 0024 D4): `graph`,
//! `context`, `brief` (whole or scoped) and `graph/{entity_id}` are
//! `specforge export` over the call's project view, through the same
//! function. The resource modules read their query through this one.
//! Nothing here reads the server's state.

use specforge_ops::read::export::{ExportField, Format, Request};

use crate::resource::{ReadOutcome, ResourceText};
use crate::tool::{ErrorCode, McpError};
use crate::uri_template::Matched;
use specforge_ops::view::ProjectView;

/// The query keys a graph view reads.
const KEYS: &str = "scope (or root), depth, kinds and max_tokens";

/// What a graph-view resource URI asks `ops::read::export` for: the template's
/// placeholder (or `?scope=`, alias `?root=`, on an untemplated URI) as the
/// scope, `depth`, `kinds` (a kinds list, `specforge_ops::view::kinds_of_text`) and `max_tokens`. Keys and
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
    /// The query of `uri`; `scope` names the template's placeholder whose
    /// value is the scope (`"entity_id"`), or is None on an untemplated URI.
    pub fn parse(uri: &Matched<'_>, scope: Option<&'static str>) -> Result<Self, Box<McpError>> {
        let query = uri.query();
        // An untemplated view names `scope` until a query gives the scope under another key.
        let mut parsed = ViewQuery {
            scope_key: scope.unwrap_or("scope"),
            ..ViewQuery::default()
        };
        if let Some(name) = scope {
            parsed.scope = uri.value(name).map(str::to_string);
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
                    if scope.is_some() {
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
                    parsed.kinds = specforge_ops::view::kinds_of_text(&value)
                        .map(str::to_string)
                        .collect();
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
            ExportField::MaxTokens => Some("max_tokens"),
            ExportField::SchemaVersion | ExportField::Schema | ExportField::Directory => None,
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

/// A query the resource cannot read: `invalid_input` naming `key`.
fn refuse(key: &str, message: String) -> Box<McpError> {
    Box::new(McpError::new(ErrorCode::InvalidInput, message).with_argument(key))
}

/// `text` with its percent-escapes decoded; `what` names it when the
/// escapes are malformed or decode to no UTF-8.
pub(super) fn decode(what: &str, text: &str) -> Result<String, Box<McpError>> {
    crate::uri_template::decode(text)
        .ok_or_else(|| refuse(what, format!("'{text}' is not valid percent-encoded UTF-8")))
}

/// A resource that takes no query: any is refused naming its first key.
pub(super) fn no_query(uri: &Matched<'_>) -> Result<(), Box<McpError>> {
    let query = uri.query();
    if query.is_empty() {
        return Ok(());
    }
    let key = query.split(['&', '=']).next().unwrap_or_default();
    Err(refuse(
        key,
        format!("this resource reads no query: '{key}' is not read"),
    ))
}

/// A graph view (`graph`, `context`, `brief`, with or without a scope): the
/// text `specforge export` writes for the same request, through the same
/// function over the call's project view (ADR 0004 D3-a). A failure is the
/// operation's McpError.
pub(crate) fn export_view(
    view: ProjectView<'_>,
    uri: &Matched<'_>,
    format: Format,
    scope: Option<&'static str>,
) -> ReadOutcome {
    let query = ViewQuery::parse(uri, scope)?;
    if scope.is_some() {
        check_entity_id(query.scope.as_deref().unwrap_or_default())?;
    }
    exported(view, &query, format)
}

/// An entity ID the URI template read (non-empty, one segment): told apart,
/// when it cannot be one (the 400 case: no identifier nor scheme ref ID, as the
/// grammar reads them), from a well-formed one that names no entity (the 404).
pub(super) fn check_entity_id(entity_id: &str) -> Result<(), Box<McpError>> {
    if specforge_parser::lex::is_entity_id(entity_id) {
        return Ok(());
    }
    Err(refuse(
        "entity_id",
        format!(
            "Malformed entity_id: {entity_id:?} is no entity ID: an identifier (letters, digits and '_', \
             not starting with a digit) or a scheme ref ID (scheme.kind:id)"
        ),
    ))
}

pub(super) fn exported(view: ProjectView<'_>, query: &ViewQuery, format: Format) -> ReadOutcome {
    specforge_ops::read::export::export(&view, &query.request(format))
        .map(ResourceText::noticed)
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
        let key = |template: &'static str, uri: &str, scope: Option<&'static str>| {
            let matched = crate::uri_template::read(template, uri).unwrap();
            ViewQuery::parse(&matched, scope)
                .unwrap()
                .argument(ExportField::Scope)
        };
        let graph = "specforge://graph";
        assert_eq!(
            key(graph, "specforge://graph?root=ghost", None),
            Some("root")
        );
        assert_eq!(
            key(graph, "specforge://graph?scope=ghost", None),
            Some("scope")
        );
        assert_eq!(
            key(
                "specforge://graph/{entity_id}",
                "specforge://graph/ghost",
                Some("entity_id")
            ),
            Some("entity_id")
        );
        let matched = crate::uri_template::read(graph, "specforge://graph?max_tokens=5").unwrap();
        let query = ViewQuery::parse(&matched, None).unwrap();
        assert_eq!(query.argument(ExportField::MaxTokens), Some("max_tokens"));
        assert_eq!(query.argument(ExportField::Scope), Some("scope"));
        assert_eq!(query.argument(ExportField::SchemaVersion), None);
    }

    #[specforge_test_macros::test(
        behavior = "serve_graph_resource",
        verify = "a graph view's kinds query is a kinds list: the items between commas, each trimmed, an empty item kept"
    )]
    fn a_graph_views_kinds_are_a_kinds_list() {
        let kinds = |uri: &str| {
            let matched = crate::uri_template::read("specforge://brief", uri).unwrap();
            ViewQuery::parse(&matched, None).unwrap().kinds
        };
        assert_eq!(
            kinds("specforge://brief?kinds=type,%20event"),
            ["type", "event"]
        );
        assert_eq!(
            kinds("specforge://brief?kinds=type%2C%20event"),
            ["type", "event"]
        );
        assert_eq!(kinds("specforge://brief?kinds=type,"), ["type", ""]);
        assert_eq!(kinds("specforge://brief?kinds="), [""]);
    }
}
