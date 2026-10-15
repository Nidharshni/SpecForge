//! Which URIs a resource's URI or RFC 6570 template names, and what a core
//! template's placeholders hold.
//!
//! Naming is one rule for core and extension resources (ADR 0049 D7): a
//! template names every URI that starts with its text before the first
//! placeholder. Reading is a core template's (ADR 0103 D2): each
//! `{placeholder}` is one non-empty, percent-decoded path segment, and any
//! other value is [`Malformed`], naming the placeholder. A query is the
//! reader's to read or refuse (ADR 0024 D4).

use crate::tool::{ErrorCode, McpError};

/// Whether `template` names `uri`. A template with a `{placeholder}` names
/// every URI that starts with its text before the first placeholder: what
/// stands for the placeholders, empty included, is [`read`]'s to accept or
/// refuse (`specforge://graph/` is named, then refused as a malformed
/// `entity_id`, never "Unknown resource URI"). A URI without one names
/// itself, alone or followed by a query (`?...`), which the reader reads or
/// refuses (ADR 0024 D4).
pub(crate) fn names(template: &str, uri: &str) -> bool {
    match template.split_once('{') {
        Some((head, _)) => uri.starts_with(head),
        None => uri
            .strip_prefix(template)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('?')),
    }
}

/// What a core template's placeholders hold in a URI it names, and the
/// query that follows.
#[derive(Debug)]
pub struct Matched<'u> {
    values: Vec<(&'static str, String)>,
    query: &'u str,
}

impl<'u> Matched<'u> {
    /// The value of `placeholder`: one non-empty, percent-decoded segment.
    /// None for a name the template does not have.
    pub fn value(&self, placeholder: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(name, _)| *name == placeholder)
            .map(|(_, value)| value.as_str())
    }

    /// The query (what follows the first `?`), raw; empty when there is none.
    pub fn query(&self) -> &'u str {
        self.query
    }
}

/// A placeholder's value a core template refuses.
#[derive(Debug)]
pub struct Malformed {
    pub placeholder: &'static str,
    pub message: String,
}

impl Malformed {
    /// `invalid_input` naming the placeholder (the envelope adds the URI).
    pub(crate) fn refusal(self) -> Box<McpError> {
        Box::new(
            McpError::new(ErrorCode::InvalidInput, self.message).with_argument(self.placeholder),
        )
    }
}

/// Read `uri` over `template`: each `{placeholder}` is the characters up to
/// the next `/` (or the end of the path), which must be non-empty, valid
/// percent-encoded UTF-8, and followed by what the template says (no more
/// segments). Precondition: `names(template, uri)`.
pub fn read<'u>(template: &'static str, uri: &'u str) -> Result<Matched<'u>, Malformed> {
    let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
    let mut values: Vec<(&'static str, String)> = Vec::new();
    let mut rest = path;
    let mut tail = template;
    // The last placeholder read and the text it read, for the refusal of
    // what follows it when that is not what the template says.
    let mut last: Option<(&'static str, &str)> = None;
    while let Some((literal, after)) = tail.split_once('{') {
        let (name, after) = after.split_once('}').unwrap_or((after, ""));
        let Some(here) = rest.strip_prefix(literal) else {
            return Err(spans_segments(last, rest));
        };
        let (raw, after_raw) = here.split_at(here.find('/').unwrap_or(here.len()));
        if raw.is_empty() {
            return Err(Malformed {
                placeholder: name,
                message: format!("Malformed {name}: must not be empty"),
            });
        }
        let Some(value) = decode(raw) else {
            return Err(Malformed {
                placeholder: name,
                message: format!("'{raw}' is not valid percent-encoded UTF-8"),
            });
        };
        values.push((name, value));
        last = Some((name, raw));
        rest = after_raw;
        tail = after;
    }
    if rest != tail {
        return Err(spans_segments(last, rest));
    }
    Ok(Matched { values, query })
}

/// What follows the last placeholder read is not what the template says:
/// the placeholder spans more than one segment.
fn spans_segments(last: Option<(&'static str, &str)>, rest: &str) -> Malformed {
    let (name, raw) = last.unwrap_or_default();
    Malformed {
        placeholder: name,
        message: format!("Malformed {name}: \"{raw}{rest}\" is more than one path segment"),
    }
}

/// `text` with its percent-escapes decoded (RFC 3986 §2.1); None when the
/// escapes are malformed or decode to no UTF-8.
pub(crate) fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// A URI `template` names: each placeholder filled with `x`. What decides
/// whether a resource another one serves shadows it (ADR 0017 D9).
pub(crate) fn probe(template: &str) -> String {
    let mut probe = String::new();
    let mut rest = template;
    while let Some((head, tail)) = rest.split_once('{') {
        probe.push_str(head);
        probe.push('x');
        rest = tail.split_once('}').map_or("", |(_, after)| after);
    }
    probe.push_str(rest);
    probe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[specforge_test_macros::test(
        behavior = "dispatch_surface_mcp_resource",
        verify = "a URI names a core or extension resource by one rule: a template's text before its first placeholder and anything after it, a plain URI alone or followed by a query"
    )]
    fn a_template_names_every_uri_after_its_head() {
        let template = "specforge://graph/{entity_id}";
        assert!(names(template, "specforge://graph/a"));
        assert!(names(template, "specforge://graph/"));
        assert!(names(template, "specforge://graph/a?depth=2"));
        assert!(!names(template, "specforge://graph"));
        assert!(!names(template, "specforge://grap"));
    }

    #[specforge_test_macros::test(
        behavior = "dispatch_surface_mcp_resource",
        verify = "a URI names a core or extension resource by one rule: a template's text before its first placeholder and anything after it, a plain URI alone or followed by a query"
    )]
    fn a_plain_uri_names_itself_and_its_queries() {
        let uri = "specforge://diagnostics";
        assert!(names(uri, "specforge://diagnostics"));
        assert!(names(uri, "specforge://diagnostics?x=1"));
        assert!(!names(uri, "specforge://diagnostics/x"));
        assert!(!names(uri, "specforge://diagnosticsx"));
    }

    #[specforge_test_macros::test(
        behavior = "dispatch_surface_mcp_resource",
        verify = "a URI names a core or extension resource by one rule: a template's text before its first placeholder and anything after it, a plain URI alone or followed by a query"
    )]
    fn a_probe_fills_each_placeholder() {
        assert_eq!(probe("acme://{a}/b/{c}"), "acme://x/b/x");
        assert_eq!(probe("specforge://graph"), "specforge://graph");
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_resources",
        verify = "a core resource template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as invalid input naming it"
    )]
    fn a_placeholder_reads_one_segment() {
        let graph = "specforge://graph/{entity_id}";
        let plain = read(graph, "specforge://graph/a").unwrap();
        assert_eq!(plain.value("entity_id"), Some("a"));
        let queried = read(graph, "specforge://graph/a?depth=2").unwrap();
        assert_eq!(queried.value("entity_id"), Some("a"));
        assert_eq!(queried.query(), "depth=2");
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_resources",
        verify = "a core resource template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as invalid input naming it"
    )]
    fn an_empty_placeholder_is_malformed() {
        let err = read("specforge://graph/{entity_id}", "specforge://graph/").unwrap_err();
        assert_eq!(err.placeholder, "entity_id");
        assert_eq!(err.message, "Malformed entity_id: must not be empty");
        let err = read("specforge://entities/{kind}", "specforge://entities/?x=1").unwrap_err();
        assert_eq!(err.placeholder, "kind");
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_resources",
        verify = "a core resource template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as invalid input naming it"
    )]
    fn a_placeholder_of_two_segments_is_malformed() {
        let err = read("specforge://graph/{entity_id}", "specforge://graph/alpha/x").unwrap_err();
        assert_eq!(err.placeholder, "entity_id");
        assert_eq!(
            err.message,
            "Malformed entity_id: \"alpha/x\" is more than one path segment"
        );
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_resources",
        verify = "a core resource template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as invalid input naming it"
    )]
    fn a_placeholder_is_percent_decoded() {
        let kind = read(
            "specforge://entities/{kind}",
            "specforge://entities/behavior%2Fextra",
        )
        .unwrap();
        assert_eq!(kind.value("kind"), Some("behavior/extra"));
        let err = read("specforge://graph/{entity_id}", "specforge://graph/%ZZ").unwrap_err();
        assert_eq!(err.placeholder, "entity_id");
        assert!(err.message.contains("percent-encoded"), "{}", err.message);
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_resources",
        verify = "a core resource template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as invalid input naming it"
    )]
    fn a_plain_uri_reads_its_query() {
        let matched = read("specforge://graph", "specforge://graph?scope=a").unwrap();
        assert_eq!(matched.value("scope"), None);
        assert_eq!(matched.query(), "scope=a");
    }

    #[specforge_test_macros::test(
        behavior = "list_mcp_resources",
        verify = "a core resource template reads each placeholder as one non-empty, percent-decoded path segment and refuses any other value as invalid input naming it"
    )]
    fn two_placeholders_read_in_order() {
        let template = "acme://{a}/b/{c}";
        let matched = read(template, "acme://x/b/y").unwrap();
        assert_eq!(matched.value("a"), Some("x"));
        assert_eq!(matched.value("c"), Some("y"));
        let err = read(template, "acme://x/c/y").unwrap_err();
        assert_eq!(err.placeholder, "a");
    }
}
