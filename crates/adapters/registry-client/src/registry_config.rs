use serde::{Deserialize, Serialize};
use specforge_common::{Diagnostic, codes};
use specforge_registry_wire::RegistryRoot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistryConfig {
    pub alias: String,
    /// The registry's root (an http or https URL without the protocol's `/v1`).
    pub url: RegistryRoot,
    #[serde(default)]
    pub scope_filter: Option<String>,
    #[serde(default)]
    pub default_registry: bool,
}

/// The token a request to one registry carries, resolved from where the user keeps it
/// ([`crate::CredentialStore::credential`], or `SPECFORGE_REGISTRY_TOKEN` for a publish). The client
/// sends it as `Authorization: Bearer`; it reads no variable or file itself. `Debug` never shows the
/// token.
#[derive(Clone, PartialEq, Eq)]
pub struct RegistryCredential {
    /// The registry root it was kept for (`RegistryConfig::url`); a client sends it nowhere else.
    pub root: RegistryRoot,
    token: String,
}

impl RegistryCredential {
    pub fn new(root: RegistryRoot, token: impl Into<String>) -> Self {
        Self {
            root,
            token: token.into(),
        }
    }

    /// Whether this credential may go with a request to `registry`: only its own root.
    pub fn is_for(&self, registry: &RegistryConfig) -> bool {
        self.root == registry.url
    }

    /// The raw token, for the `Authorization` header only.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl std::fmt::Debug for RegistryCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryCredential")
            .field("root", &self.root)
            .field("token", &"****")
            .finish()
    }
}

/// Parses the `"registries"` array from a JSON config string.
///
/// Returns a tuple of (parsed registries, diagnostics).
/// Produces an I003 info diagnostic when no default registry is configured.
/// Produces a W-level diagnostic for duplicate aliases.
/// An entry that can't be read (its `url` isn't a registry root included) is E067 and is left out;
/// the caller refuses the configuration (`specforge_ops_registry::configured`).
pub fn parse_registries_from_config(config_json: &str) -> (Vec<RegistryConfig>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();

    let parsed: serde_json::Value = match serde_json::from_str(config_json) {
        Ok(v) => v,
        Err(e) => {
            diagnostics.push(
                Diagnostic::new(
                    codes::E067,
                    format!("Failed to parse registry config JSON: {e}"),
                )
                .with_suggestion("Ensure the configuration is valid JSON.".to_string()),
            );
            return (Vec::new(), diagnostics);
        }
    };

    let registries_value = match parsed.get("registries") {
        Some(v) => v,
        None => {
            diagnostics.push(
                Diagnostic::new(
                    codes::I003,
                    "No registries configured and no default registry set.".to_string(),
                )
                .with_suggestion("Add a \"registries\" array to your configuration.".to_string()),
            );
            return (Vec::new(), diagnostics);
        }
    };

    let registries_array = match registries_value.as_array() {
        Some(arr) => arr,
        None => {
            diagnostics.push(Diagnostic::new(
                codes::E067,
                "\"registries\" must be a JSON array.".to_string(),
            ));
            return (Vec::new(), diagnostics);
        }
    };

    let mut registries = Vec::new();
    let mut seen_aliases = std::collections::HashSet::new();

    for (i, entry) in registries_array.iter().enumerate() {
        match serde_json::from_value::<RegistryConfig>(entry.clone()) {
            Ok(reg) => {
                if !seen_aliases.insert(reg.alias.clone()) {
                    diagnostics.push(
                        Diagnostic::new(
                            codes::W140,
                            format!("Duplicate registry alias \"{}\" at index {i}.", reg.alias),
                        )
                        .with_suggestion("Use unique aliases for each registry.".to_string()),
                    );
                }
                registries.push(reg);
            }
            Err(e) => {
                diagnostics.push(Diagnostic::new(
                    codes::E067,
                    format!("Failed to parse registry entry at index {i}: {e}"),
                ));
            }
        }
    }

    let has_default = registries.iter().any(|r| r.default_registry);
    if registries.is_empty() || !has_default {
        diagnostics.push(
            Diagnostic::new(
                codes::I003,
                if registries.is_empty() {
                    "No registries configured and no default registry set.".to_string()
                } else {
                    "No registry is the default: a package no \"scope_filter\" matches has no registry (R-OPS-001)."
                        .to_string()
                },
            )
            .with_suggestion(
                "Set \"default_registry\": true on one of your registries.".to_string(),
            ),
        );
    }

    (registries, diagnostics)
}
