//! Which URIs a resource's URI or RFC 6570 template names: one rule for
//! core and extension resources (ADR 0049 D7). What a URI holds beyond what
//! the rule reads (a placeholder's value, a query) is the reader's to read
//! or refuse.

/// Whether `template` names `uri`. A template with a `{placeholder}` names
/// every URI that starts with its text before the first placeholder: what
/// stands for the placeholders, empty included, is the reader's (the entity
/// resource refuses `specforge://graph/` as a malformed ID). A URI without
/// one names itself, alone or followed by a query (`?...`), which the reader
/// reads or refuses (ADR 0024 D4).
pub(crate) fn names(template: &str, uri: &str) -> bool {
    match template.split_once('{') {
        Some((head, _)) => uri.starts_with(head),
        None => uri
            .strip_prefix(template)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('?')),
    }
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
}
