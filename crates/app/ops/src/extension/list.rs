//! `specforge extensions` / `specforge providers` and their MCP tools:
//! management operations over the project view (ADR 0015, "Management
//! operations"). Both read the config the compile read, from the view's
//! environment, never `specforge.json` again.

use crate::view::ProjectView;
use serde::Serialize;
use specforge_common::DiagnosticList;
use specforge_common::shape::Shape;
use std::collections::BTreeSet;

pub use super::Status;

/// One extension as both surfaces print it (`McpExtensionInfo`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ExtensionInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `builtin`, the lock entry's source, `file:<path>` or `unknown`.
    pub source: String,
    pub status: Status,
    /// The entity kinds it contributes, sorted.
    pub entity_kinds: Vec<String>,
    pub entity_count: usize,
    pub validation_rules: usize,
}

/// One entry of the project's `specforge.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Shape)]
pub struct LockedExtension {
    pub name: String,
    pub version: String,
}

/// What `specforge extensions --format json` and `specforge.extensions`
/// print (`McpExtensionsResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ExtensionsDocument {
    /// Sorted by name.
    pub extensions: Vec<ExtensionInfo>,
    /// How many extensions are listed.
    pub count: usize,
    /// The lock's entries, in lock order.
    pub lock_file_entries: Vec<LockedExtension>,
    /// The kinds of the graph's entities.
    pub entity_kinds_in_graph: BTreeSet<String>,
}

/// Every extension of the project ([`super::inventory`]), sorted by name,
/// with the entity kinds each registered (from the kind registry), how many
/// of the graph's entities use them, and its rule count; the lock's
/// entries; the kinds its graph uses: the document both surfaces print. The
/// lock is the view's ([`ProjectView::lock`]): none without a root.
pub fn list(view: &ProjectView) -> ExtensionsDocument {
    let kinds = view.registries().kinds();
    let extensions: Vec<ExtensionInfo> = super::inventory(view)
        .rows()
        .iter()
        .map(|extension| {
            let mut entity_kinds: Vec<String> = kinds
                .iter()
                .filter(|(_, entry)| entry.source_extension == extension.name)
                .map(|(kind, _)| kind.clone())
                .collect();
            entity_kinds.sort();
            let entity_count = super::entities_of(view.graph(), kinds, extension.name).count();
            ExtensionInfo {
                name: extension.name.to_string(),
                version: extension.version().map(str::to_string),
                source: extension.origin.source(),
                status: extension.status,
                entity_kinds,
                entity_count,
                validation_rules: extension
                    .declaration
                    .map_or(0, |d| d.validation_rules.len()),
            }
        })
        .collect();

    ExtensionsDocument {
        count: extensions.len(),
        extensions,
        lock_file_entries: view
            .lock()
            .entries()
            .iter()
            .map(|e| LockedExtension {
                name: e.name.to_string(),
                version: e.version.clone(),
            })
            .collect(),
        entity_kinds_in_graph: view
            .graph()
            .nodes()
            .iter()
            .map(|n| n.kind.raw.to_string())
            .collect(),
    }
}

pub use specforge_project::providers::ProviderStatus;

/// The providers listing as both surfaces answer it (`McpProvidersResult`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ProvidersDocument {
    pub providers: Vec<ProviderRow>,
    pub count: usize,
    pub diagnostics: DiagnosticList,
}

/// One configured provider.
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ProviderRow {
    pub scheme: String,
    pub alias: String,
    pub extension: String,
    pub status: ProviderStatus,
}

/// The providers the view's environment registered when the project was
/// compiled, in declaration order (the first to declare a scheme wins it),
/// each with its status, and the diagnostics reading and registering them
/// produced (W118 for a malformed entry or an extension that is not loaded
/// or contributes no providers, E057 for a scheme declared twice): the
/// document both surfaces answer with. A provider's settings are the
/// environment's and are not listed. Never `specforge.json` again.
pub fn providers(view: &ProjectView) -> ProvidersDocument {
    let registered = view.env().providers();
    let providers: Vec<ProviderRow> = registered
        .iter()
        .map(|p| ProviderRow {
            scheme: p.scheme.to_string(),
            alias: p.alias.to_string(),
            extension: p.extension.to_string(),
            status: p.status,
        })
        .collect();
    ProvidersDocument {
        count: providers.len(),
        providers,
        diagnostics: DiagnosticList(registered.diagnostics().cloned().collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Viewed;
    use serde_json::json;
    use specforge_project::EnabledExtension;
    use specforge_project::testing::Assembled;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "the extensions listing reads the config entries from the view, never specforge.json again"
    )]
    fn the_listing_reads_the_config_from_the_view_not_from_disk() {
        let fixture = Assembled::new().config(&["@acme/missing@1.2.0"]);
        std::fs::write(
            fixture.root().join("specforge.json"),
            r#"{"extensions": ["@other/x@9.9.9"]}"#,
        )
        .unwrap();

        let listing = list(&fixture.view());

        let names: Vec<&str> = listing.extensions.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["@acme/missing"]);
        let missing = &listing.extensions[0];
        assert_eq!(missing.version.as_deref(), Some("1.2.0"));
        assert_eq!(missing.status, Status::NotLoaded);
        assert_eq!(missing.source, "unknown");
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "list, doctor and remove read the lock the compile read, once"
    )]
    fn the_listing_reads_the_lock_the_compile_read_not_the_disk() {
        let fixture = Assembled::new().lock(&[("@acme/locked", "1.0.0", "registry")]);
        // The file changed after the compile read it.
        std::fs::remove_file(specforge_installed::lock_path(fixture.root())).unwrap();

        let listing = list(&fixture.view());

        assert_eq!(
            listing.lock_file_entries,
            [LockedExtension {
                name: "@acme/locked".into(),
                version: "1.0.0".into()
            }]
        );
    }

    #[specforge_test(
        behavior = "list_installed_extensions",
        verify = "a .wasm file entry is listed under the name it declares, loaded, with source file:<path>"
    )]
    fn a_file_entry_is_listed_with_its_file_origin() {
        let fixture = Assembled::new()
            .enabled(vec![EnabledExtension {
                entry: "greet.wasm".into(),
                name: "@sdk/greet".into(),
                file: Some("greet.wasm".into()),
                failure: None,
                warnings: Vec::new(),
            }])
            .declarations(vec![Assembled::declaration("@sdk/greet", "0.1.0")]);

        let listing = list(&fixture.view());

        assert_eq!(listing.extensions.len(), 1, "{listing:?}");
        let greet = &listing.extensions[0];
        assert_eq!(greet.name, "@sdk/greet");
        assert_eq!(greet.source, "file:greet.wasm");
        assert_eq!(greet.status, Status::Loaded);
        assert_eq!(greet.version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn a_rootless_view_lists_what_it_enabled_and_loaded() {
        let fixture = Assembled::new()
            .config(&["@specforge/product"])
            .declarations(vec![Assembled::declaration("@acme/loaded", "2.0.0")])
            .lock(&[("@acme/locked", "1.0.0", "registry")]);

        let rootless = list(&fixture.rootless_view());

        let listed: Vec<(&str, Status)> = rootless
            .extensions
            .iter()
            .map(|e| (e.name.as_str(), e.status))
            .collect();
        assert_eq!(
            listed,
            [
                ("@acme/loaded", Status::NotConfigured),
                ("@specforge/product", Status::NotLoaded),
            ]
        );
        assert!(
            rootless.lock_file_entries.is_empty(),
            "no lock is read without a root"
        );

        // Rooted, the lock is read at the root.
        let rooted = list(&fixture.view());
        assert_eq!(
            rooted.lock_file_entries,
            [LockedExtension {
                name: "@acme/locked".into(),
                version: "1.0.0".into()
            }]
        );
        let locked = rooted
            .extensions
            .iter()
            .find(|e| e.name == "@acme/locked")
            .unwrap();
        assert_eq!(locked.source, "registry");
    }

    #[specforge_test(
        behavior = "list_configured_providers",
        verify = "list shows all configured providers"
    )]
    fn the_providers_listing_reads_the_registration_from_the_view() {
        let fixture = Assembled::new().config_json(json!({
            "extensions": [],
            "providers": [{"scheme": "gh", "alias": "work", "extension": "@acme/issues"}],
        }));
        // What is on disk says something else: the view's registration is read.
        std::fs::write(fixture.root().join("specforge.json"), "{}").unwrap();

        let listing = providers(&fixture.view());

        assert_eq!(listing.providers.len(), 1, "{listing:?}");
        let entry = &listing.providers[0];
        assert_eq!(
            (entry.scheme.as_str(), entry.alias.as_str()),
            ("gh", "work")
        );
        assert_eq!(entry.extension, "@acme/issues");
        assert_eq!(entry.status.as_str(), "extension_not_loaded");
        let codes: Vec<&str> = listing
            .diagnostics
            .0
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W118"], "{listing:?}");
        assert_eq!(listing.count, 1);
    }
}
