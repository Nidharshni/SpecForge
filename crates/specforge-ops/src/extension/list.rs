//! `specforge extensions` / `specforge providers` and their MCP tools:
//! management operations over the project view (ADR 0015, "Management
//! operations"). Both read the config the compile read, from the view's
//! environment, never `specforge.json` again.

use super::Origin;
use crate::view::ProjectView;
use serde::Serialize;
use specforge_common::shape::Shape;
use specforge_common::{Diagnostic, DiagnosticList, ExtensionEntry as Entry};
use std::collections::BTreeSet;

/// Whether an extension listed is part of the compiled project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Shape)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Enabled in `specforge.json` and loaded.
    Loaded,
    /// Enabled but not loaded (not installed, or it failed to load).
    NotLoaded,
    /// Installed (locked) or loaded, but no longer enabled.
    NotConfigured,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Loaded => "loaded",
            Status::NotLoaded => "not_loaded",
            Status::NotConfigured => "not_configured",
        }
    }
}

/// One extension of the project: what it is and what it contributes.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionEntry {
    pub name: String,
    /// The loaded version, else the locked one, else the version a legacy
    /// `name@version` entry writes.
    pub version: Option<String>,
    pub origin: Origin,
    pub status: Status,
    /// The entity kinds it contributes, sorted.
    pub entity_kinds: Vec<String>,
    /// How many of the project's entities are of those kinds.
    pub entity_count: usize,
    pub validation_rules: usize,
}

impl ExtensionEntry {
    /// `{name, version, source, status, entity_kinds, entity_count,
    /// validation_rules}`: the entry both surfaces print.
    pub fn info(&self) -> ExtensionInfo {
        ExtensionInfo {
            name: self.name.clone(),
            version: self.version.clone(),
            source: self.origin.source(),
            status: self.status,
            entity_kinds: self.entity_kinds.clone(),
            entity_count: self.entity_count,
            validation_rules: self.validation_rules,
        }
    }
}

/// One extension as both surfaces print it (`McpExtensionInfo`).
#[derive(Debug, Clone, PartialEq, Serialize, Shape)]
pub struct ExtensionInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `builtin`, the lock entry's source, or `file:<path>`.
    pub source: String,
    pub status: Status,
    /// The entity kinds it contributes, sorted.
    pub entity_kinds: Vec<String>,
    pub entity_count: usize,
    pub validation_rules: usize,
}

/// One entry of the project's `specforge.lock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedExtension {
    pub name: String,
    pub version: String,
}

/// Every extension the project enables, has locked at its root, or
/// loaded; the lock's entries; the kinds its graph uses.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionListing {
    /// Sorted by name.
    pub extensions: Vec<ExtensionEntry>,
    /// In lock order; empty without a lock or a root.
    pub locked: Vec<LockedExtension>,
    /// The kinds of the graph's entities.
    pub kinds_in_graph: BTreeSet<String>,
}

/// Every extension the project enables (its view's `env.enabled`), has
/// locked at its root, or loaded, sorted by name, with the entity kinds
/// each registered (from the kind registry), how many of the graph's
/// entities use them, and its rule count; the lock's entries; the kinds
/// its graph uses. The lock is the view's ([`ProjectView::lock`]): none
/// without a root.
pub fn list(view: &ProjectView) -> ExtensionListing {
    let enabled = &view.env().enabled;
    let entries = &view.env().config.extensions;
    let loaded = view.registries().declarations();
    let kinds = &view.registries().kinds;
    // The version a legacy `name@version` entry names.
    let configured_version = |name: &str| {
        entries.iter().find_map(|e| {
            let e = e.trim();
            (Entry::parse(e) == Entry::Named(name) && e.len() > name.len())
                .then(|| e[name.len() + 1..].to_string())
        })
    };
    let lock = view.lock().file();
    let lock_entries = view.lock().entries();
    let enabled_names: Vec<&str> = enabled.iter().map(|e| e.name.as_str()).collect();

    let mut names: Vec<String> = enabled_names
        .iter()
        .map(|name| name.to_string())
        .chain(lock_entries.iter().map(|e| e.name.to_string()))
        .chain(loaded.iter().map(|d| d.name().to_string()))
        .collect();
    names.sort();
    names.dedup();

    let extensions = names
        .into_iter()
        .map(|name| {
            let declaration = loaded.iter().find(|d| d.name() == name);
            let locked = lock_entries.iter().find(|e| e.name.as_str() == name);
            let status = match (enabled_names.contains(&name.as_str()), declaration) {
                (true, Some(_)) => Status::Loaded,
                (true, None) => Status::NotLoaded,
                (false, _) => Status::NotConfigured,
            };
            let mut entity_kinds: Vec<String> = kinds
                .iter()
                .filter(|(_, entry)| entry.source_extension == name)
                .map(|(kind, _)| kind.clone())
                .collect();
            entity_kinds.sort();
            let entity_count = view
                .graph()
                .nodes()
                .iter()
                .filter(|n| entity_kinds.iter().any(|k| k == n.kind.raw.as_str()))
                .count();
            ExtensionEntry {
                version: declaration
                    .map(|d| d.version().to_string())
                    .or_else(|| locked.map(|e| e.version.clone()))
                    .or_else(|| configured_version(&name)),
                validation_rules: declaration.map_or(0, |d| d.validation_rules.len()),
                origin: Origin::of(&name, enabled, lock),
                name,
                status,
                entity_kinds,
                entity_count,
            }
        })
        .collect();

    ExtensionListing {
        extensions,
        locked: lock_entries
            .iter()
            .map(|e| LockedExtension {
                name: e.name.to_string(),
                version: e.version.clone(),
            })
            .collect(),
        kinds_in_graph: view
            .graph()
            .nodes()
            .iter()
            .map(|n| n.kind.raw.to_string())
            .collect(),
    }
}

/// One configured provider, as the environment registered it.
pub use specforge_project::providers::Provider as ProviderEntry;
pub use specforge_project::providers::ProviderStatus;

/// The providers the view's config configures, in declaration order, with
/// their status, and what loading and registering them reported.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderListing {
    pub providers: Vec<ProviderEntry>,
    /// W118 (a malformed entry, or an extension that is not loaded or
    /// contributes no providers) and E057 (a scheme declared twice).
    pub diagnostics: Vec<Diagnostic>,
}

impl ProviderListing {
    /// `{providers, count, diagnostics}`: the document both surfaces
    /// answer with.
    pub fn document(&self) -> ProvidersDocument {
        ProvidersDocument {
            count: self.providers.len(),
            providers: self
                .providers
                .iter()
                .map(|p| ProviderRow {
                    scheme: p.scheme.to_string(),
                    alias: p.alias.to_string(),
                    extension: p.extension.to_string(),
                    status: p.status,
                })
                .collect(),
            diagnostics: DiagnosticList(self.diagnostics.clone()),
        }
    }
}

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
/// produced (W118, E057). Never `specforge.json` again.
pub fn providers(view: &ProjectView) -> ProviderListing {
    let registered = &view.env().providers;
    ProviderListing {
        providers: registered.iter().cloned().collect(),
        diagnostics: registered.diagnostics().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::testing::Fixture;
    use serde_json::json;
    use specforge_installed::LockSource;
    use specforge_project::EnabledExtension;
    use specforge_test_macros::test as specforge_test;

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "the extensions listing reads the config entries from the view, never specforge.json again"
    )]
    fn the_listing_reads_the_config_from_the_view_not_from_disk() {
        let fixture = Fixture::new().config(&["@acme/missing@1.2.0"]);
        std::fs::write(
            fixture.dir.path().join("specforge.json"),
            r#"{"extensions": ["@other/x@9.9.9"]}"#,
        )
        .unwrap();

        let listing = list(&fixture.view());

        let names: Vec<&str> = listing.extensions.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["@acme/missing"]);
        let missing = &listing.extensions[0];
        assert_eq!(missing.version.as_deref(), Some("1.2.0"));
        assert_eq!(missing.status, Status::NotLoaded);
        assert_eq!(missing.origin, Origin::Unknown);
        assert_eq!(missing.origin.source(), "unknown");
    }

    #[specforge_test(
        behavior = "management_operations_over_the_project_view",
        verify = "list, doctor and remove read the lock the compile read, once"
    )]
    fn the_listing_reads_the_lock_the_compile_read_not_the_disk() {
        let fixture = Fixture::new().lock(&[("@acme/locked", "1.0.0", "registry")]);
        // The file changed after the compile read it.
        std::fs::remove_file(specforge_installed::lock_path(fixture.dir.path())).unwrap();

        let listing = list(&fixture.view());

        assert_eq!(
            listing.locked,
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
        let fixture = Fixture::new()
            .enabled(vec![EnabledExtension {
                entry: "greet.wasm".into(),
                name: "@sdk/greet".into(),
                file: Some("greet.wasm".into()),
                failure: None,
            }])
            .declarations(vec![Fixture::declaration("@sdk/greet", "0.1.0")]);

        let listing = list(&fixture.view());

        assert_eq!(listing.extensions.len(), 1, "{listing:?}");
        let greet = &listing.extensions[0];
        assert_eq!(greet.name, "@sdk/greet");
        assert_eq!(
            greet.origin,
            Origin::File {
                path: "greet.wasm".into()
            }
        );
        assert_eq!(greet.origin.source(), "file:greet.wasm");
        assert_eq!(greet.status, Status::Loaded);
        assert_eq!(greet.version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn a_rootless_view_lists_what_it_enabled_and_loaded() {
        let fixture = Fixture::new()
            .config(&["@specforge/product"])
            .declarations(vec![Fixture::declaration("@acme/loaded", "2.0.0")])
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
        assert!(rootless.locked.is_empty(), "no lock is read without a root");

        // Rooted, the lock is read at the root.
        let rooted = list(&fixture.view());
        assert_eq!(
            rooted.locked,
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
        assert_eq!(
            locked.origin,
            Origin::Installed {
                source: LockSource::Registry
            }
        );
    }

    /// What the load records for an entry it could not load.
    fn failing(
        entry: EnabledExtension,
        problem: specforge_installed::LoadProblem,
    ) -> EnabledExtension {
        EnabledExtension {
            failure: Some(specforge_installed::LoadFailure {
                diagnostic: Diagnostic::new(
                    specforge_common::codes::E028,
                    format!("{} did not load", entry.entry),
                ),
                problem,
            }),
            ..entry
        }
    }

    #[test]
    fn pin_a_builtin_name_in_the_lock_is_named_by_its_lock_source() {
        let fixture = Fixture::new()
            .config(&["@specforge/product"])
            .declarations(vec![Fixture::declaration("@specforge/product", "1.0.0")])
            .lock(&[("@specforge/product", "9.9.9", "registry")]);

        let listing = list(&fixture.view());

        let row = &listing.extensions[0];
        assert_eq!(
            row.origin,
            Origin::Installed {
                source: LockSource::Registry
            }
        );
        assert_eq!(row.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn pin_the_source_of_a_named_and_a_file_entry_follows_entry_order() {
        use specforge_installed::LoadProblem;
        let file = failing(
            EnabledExtension::unloaded("@sdk/greet=greet.wasm"),
            LoadProblem::AlreadyLoaded {
                declared: "@sdk/greet".into(),
            },
        );
        let named = EnabledExtension::unloaded("@sdk/greet");
        let origin_of = |enabled: Vec<EnabledExtension>| {
            let fixture = Fixture::new()
                .enabled(enabled)
                .declarations(vec![Fixture::declaration("@sdk/greet", "0.1.0")])
                .lock(&[("@sdk/greet", "0.1.0", "registry")]);
            list(&fixture.view()).extensions[0].origin.clone()
        };

        assert_eq!(
            origin_of(vec![file.clone(), named.clone()]),
            Origin::File {
                path: "greet.wasm".into()
            }
        );
        assert_eq!(
            origin_of(vec![named, file]),
            Origin::Installed {
                source: LockSource::Registry
            }
        );
    }

    #[test]
    fn pin_a_failed_file_entry_takes_the_version_of_a_lock_entry_of_its_name() {
        use specforge_installed::LoadProblem;
        let fixture = Fixture::new()
            .enabled(vec![failing(
                EnabledExtension::unloaded("@test/probe=nope.wasm"),
                LoadProblem::FileMissing {
                    path: "nope.wasm".into(),
                },
            )])
            .lock(&[("@test/probe", "0.1.0", "local:/blobs/probe.wasm")]);

        let listing = list(&fixture.view());

        let row = &listing.extensions[0];
        assert_eq!(
            row.origin,
            Origin::File {
                path: "nope.wasm".into()
            }
        );
        assert_eq!(row.version.as_deref(), Some("0.1.0"));
        assert_eq!(row.status, Status::NotLoaded);
    }

    #[specforge_test(
        behavior = "list_configured_providers",
        verify = "list shows all configured providers"
    )]
    fn the_providers_listing_reads_the_registration_from_the_view() {
        let fixture = Fixture::new().config_json(json!({
            "extensions": [],
            "providers": [{"scheme": "gh", "alias": "work", "extension": "@acme/issues"}],
        }));
        // What is on disk says something else: the view's registration is read.
        std::fs::write(fixture.dir.path().join("specforge.json"), "{}").unwrap();

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
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W118"], "{listing:?}");
        assert_eq!(listing.document().count, 1);
    }
}
