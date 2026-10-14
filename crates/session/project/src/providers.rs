//! The `providers` specforge.json configures (ADR 0004 D3-c), registered
//! once per environment against the loaded declarations. The compile's I005
//! and the providers listing read this one registration, never
//! `specforge.json` again.

use std::collections::HashSet;

use serde_json::{Map, Value};
use specforge_common::{Diagnostic, codes};
use specforge_protocol_types::ExtensionDeclaration;

use crate::SetupPart;

/// Why a configured provider's scheme is registered or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specforge_common::shape::Shape)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Registered,
    /// Its extension is not loaded (not enabled, or failed to load).
    ExtensionNotLoaded,
    /// Its extension is loaded but contributes no providers.
    NotAProvider,
    /// An earlier provider already registered the scheme (E057).
    SchemeTaken,
}

impl ProviderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderStatus::Registered => "registered",
            ProviderStatus::ExtensionNotLoaded => "extension_not_loaded",
            ProviderStatus::NotAProvider => "not_a_provider",
            ProviderStatus::SchemeTaken => "scheme_taken",
        }
    }
}

/// One `providers` entry and what became of its scheme: the scheme it
/// serves, its alias, the extension that implements it, and that
/// extension's settings. Two instances of one provider use two schemes.
#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    pub scheme: String,
    pub alias: String,
    pub extension: String,
    /// The provider's own settings, as written; passed through (no host
    /// reader yet: a provider call will read them).
    pub settings: Map<String, Value>,
    pub status: ProviderStatus,
}

/// Every well-formed `providers` entry in declaration order, and what
/// reading and registering them reported.
#[derive(Debug, Clone, Default)]
pub struct Providers {
    providers: Vec<Provider>,
    /// Each with the setup part it is about: W118 is about the providers,
    /// E057 a collision.
    diagnostics: Vec<(SetupPart, Diagnostic)>,
}

fn warning(message: String, suggestion: &str) -> (SetupPart, Diagnostic) {
    (
        SetupPart::Providers,
        Diagnostic::new(codes::W118, message).with_suggestion(suggestion.to_string()),
    )
}

impl Providers {
    /// Read `config`'s `providers` (none when `config` is `None` or has no
    /// such key) and register each scheme to the extension it names among
    /// `declarations`, in declaration order: the first provider to declare
    /// a scheme wins it.
    ///
    /// Diagnostics, in order: W118 for a `providers` value that is not an
    /// array, or an entry missing scheme/alias/extension or with
    /// non-object settings (that entry is left out); then, per entry, W118
    /// when its extension is not loaded or contributes no providers, and
    /// E057 when an earlier entry took its scheme. Pure.
    pub fn register(config: Option<&Value>, declarations: &[ExtensionDeclaration]) -> Providers {
        let (entries, mut diagnostics) = read(config);
        let mut registered: Vec<(String, String, String)> = Vec::new();
        let mut providers = Vec::with_capacity(entries.len());
        for entry in entries {
            let declaration = declarations.iter().find(|d| d.name() == entry.extension);
            let status = match declaration {
                None => {
                    diagnostics.push(warning(
                        format!(
                            "provider '{}' names extension '{}', which is not loaded",
                            entry.alias, entry.extension
                        ),
                        "enable and install the extension: specforge add <extension>",
                    ));
                    ProviderStatus::ExtensionNotLoaded
                }
                Some(d) if !d.handshake.providers => {
                    diagnostics.push(warning(
                        format!(
                            "provider '{}' names extension '{}', which contributes no providers",
                            entry.alias, entry.extension
                        ),
                        "name the extension that implements this provider",
                    ));
                    ProviderStatus::NotAProvider
                }
                Some(_) => match registered
                    .iter()
                    .find(|(scheme, _, _)| *scheme == entry.scheme)
                {
                    Some((_, first_alias, first_extension)) => {
                        diagnostics.push((
                            SetupPart::Collision,
                            Diagnostic::new(
                                codes::E057,
                                format!(
                                    "scheme '{}' of provider '{}' ({}) is already registered by provider '{}' ({})",
                                    entry.scheme,
                                    entry.alias,
                                    entry.extension,
                                    first_alias,
                                    first_extension
                                ),
                            )
                            .with_suggestion(
                                "give each provider instance its own scheme".to_string(),
                            ),
                        ));
                        ProviderStatus::SchemeTaken
                    }
                    None => {
                        registered.push((
                            entry.scheme.clone(),
                            entry.alias.clone(),
                            entry.extension.clone(),
                        ));
                        ProviderStatus::Registered
                    }
                },
            };
            providers.push(Provider { status, ..entry });
        }
        Providers {
            providers,
            diagnostics,
        }
    }

    /// The configured providers, in declaration order.
    pub fn iter(&self) -> std::slice::Iter<'_, Provider> {
        self.providers.iter()
    }

    /// The registered schemes: what a `ref`'s scheme is checked against
    /// (I005).
    pub fn schemes(&self) -> HashSet<String> {
        self.providers
            .iter()
            .filter(|provider| provider.status == ProviderStatus::Registered)
            .map(|provider| provider.scheme.clone())
            .collect()
    }

    /// What reading and registering the providers reported (W118, E057).
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().map(|(_, diagnostic)| diagnostic)
    }

    /// [`Self::diagnostics`] with the setup part each is about.
    pub fn reported(&self) -> impl Iterator<Item = (SetupPart, &Diagnostic)> {
        self.diagnostics
            .iter()
            .map(|(part, diagnostic)| (*part, diagnostic))
    }
}

/// The well-formed entries of the `providers` array, each needing `scheme`,
/// `alias` and `extension` (`settings` is optional), and W118 for what was
/// left out. Their `status` is a placeholder until they are registered.
fn read(config: Option<&Value>) -> (Vec<Provider>, Vec<(SetupPart, Diagnostic)>) {
    let mut providers = Vec::new();
    let mut diagnostics = Vec::new();

    let array = match config.and_then(|config| config.get("providers")) {
        None | Some(Value::Null) => return (providers, diagnostics),
        Some(Value::Array(array)) => array,
        Some(_) => {
            diagnostics.push(warning(
                "\"providers\" must be an array of {scheme, alias, extension, settings} entries"
                    .to_string(),
                "write providers as [{\"scheme\": \"gh\", \"alias\": \"main\", \"extension\": \"@acme/gh\"}]",
            ));
            return (providers, diagnostics);
        }
    };

    for (i, entry) in array.iter().enumerate() {
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let missing: Vec<&str> = ["scheme", "alias", "extension"]
            .into_iter()
            .filter(|key| text(key).is_none())
            .collect();
        if !missing.is_empty() {
            diagnostics.push(warning(
                format!("providers[{i}]: missing {}", missing.join(", ")),
                "each provider needs a scheme, an alias and the extension that implements it",
            ));
            continue;
        }
        let settings = match entry.get("settings") {
            None => Map::new(),
            Some(Value::Object(settings)) => settings.clone(),
            Some(_) => {
                diagnostics.push(warning(
                    format!("providers[{i}]: \"settings\" must be an object"),
                    "put the provider's own settings in a \"settings\" object",
                ));
                continue;
            }
        };
        providers.push(Provider {
            scheme: text("scheme").unwrap_or_default(),
            alias: text("alias").unwrap_or_default(),
            extension: text("extension").unwrap_or_default(),
            settings,
            status: ProviderStatus::ExtensionNotLoaded,
        });
    }

    (providers, diagnostics)
}
