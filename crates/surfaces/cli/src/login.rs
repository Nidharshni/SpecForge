//! `specforge login` and `specforge logout`: a registry credential, kept
//! under the root of the registry it is for (ADR 0178).

use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::project::ProjectArg;
use crate::terminal::{Terminal, outln};
use serde_json::json;
use specforge_common::codes;
use specforge_ops::OpError;
use specforge_ops_registry::{RegistryRoot, User};
use specforge_registry_client::credentials::{CredentialEntry, CredentialStore, TokenReference};
use specforge_registry_client::{HttpRegistryClient, RegistryClient, RegistryCredential, Retrying};
use std::path::{Path, PathBuf};

// `specforge login`'s command line.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Registry alias (defaults to the default registry)
    #[arg(long)]
    registry: Option<String>,

    /// Authentication token, kept as a secret (a 0600 file under ~/.specforge/secrets).
    /// Exactly one of --token, --token-env, --token-file
    #[arg(long)]
    token: Option<String>,

    /// Environment variable that holds the token; the variable's name is kept, not the token
    #[arg(long)]
    token_env: Option<String>,

    /// File that holds the token; the file's path is kept, not the token
    #[arg(long)]
    token_file: Option<PathBuf>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

// `specforge logout`'s command line.
#[derive(clap::Args)]
pub(crate) struct LogoutArgs {
    /// Registry alias, as the project at --path names it (defaults to the default registry)
    #[arg(long, conflicts_with = "url")]
    registry: Option<String>,

    /// Registry root whose credential to forget; reads no project
    #[arg(long, value_parser = parse_root)]
    url: Option<RegistryRoot>,

    #[command(flatten)]
    project: crate::project::ProjectArg,

    #[command(flatten)]
    output: OutputArg,
}

/// A `--url` value: a registry root.
fn parse_root(text: &str) -> Result<RegistryRoot, String> {
    RegistryRoot::parse(text).map_err(|why| why.to_string())
}

impl Args {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        login(
            self.registry.as_deref(),
            &TokenSource {
                token: self.token.as_deref(),
                token_env: self.token_env.as_deref(),
                token_file: self.token_file.as_deref(),
            },
            &self.project,
            self.output.format,
            terminal,
        )
    }
}

impl LogoutArgs {
    pub(crate) fn run(self, terminal: &Terminal) -> Exit {
        logout(
            self.registry.as_deref(),
            self.url,
            &self.project,
            self.output.format,
            terminal,
        )
    }
}

/// Where the token to log in with comes from: exactly one of the three is given.
struct TokenSource<'a> {
    token: Option<&'a str>,
    token_env: Option<&'a str>,
    token_file: Option<&'a Path>,
}

/// Validate the token `source` gives against the registry `registry_alias` names (the default
/// registry when none is given) and keep it under that registry's root: `--token` as a secret
/// (a 0600 file), `--token-env` and `--token-file` as references.
fn login(
    registry_alias: Option<&str>,
    source: &TokenSource<'_>,
    project: &ProjectArg,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let refuse = |error: &OpError| Refusal::of(format).report(terminal, error);
    let given = [
        source.token.is_some(),
        source.token_env.is_some(),
        source.token_file.is_some(),
    ];
    if given.iter().filter(|g| **g).count() != 1 {
        return refuse(&OpError::coded(
            specforge_ops::OpErrorKind::InvalidInput,
            codes::R_LOGIN_001,
            "login takes exactly one token source: --token <TOKEN>, --token-env <VAR> or --token-file <PATH>",
        ));
    }

    // The token is validated against a configured registry; with none,
    // fail before any network call (ADR 0004 N1).
    let root = match project.root() {
        Ok(root) => root,
        Err(error) => return refuse(&error),
    };
    let configured = match specforge_ops_registry::configured(&root, "login") {
        Ok(configured) => configured,
        Err(error) => return refuse(&error),
    };
    Notes::new(terminal, format).beside(&configured.diagnostics);
    let registry = match configured.named(registry_alias) {
        Ok(registry) => registry.clone(),
        Err(error) => return refuse(&error),
    };
    let alias = registry.alias.as_str();
    let root = registry.url.clone();
    let user = User::current();

    // Resolve the token before any request: a reference that can't be read is R010/R011.
    let reference = match (source.token_env, source.token_file) {
        (Some(var), _) => Some(TokenReference::Env(var.to_string())),
        (_, Some(file)) => Some(TokenReference::File(file.to_path_buf())),
        _ => None,
    };
    let credential = match (&reference, source.token) {
        (_, Some(token)) => RegistryCredential::new(root.clone(), token),
        (Some(reference), None) => {
            let mut probe = CredentialStore::empty(user.dir());
            probe.keep_reference(root.clone(), reference.clone());
            match probe.credential(&root) {
                Ok(Some(credential)) => credential,
                Ok(None) => unreachable!("a reference was just set"),
                Err(diag) => return refuse(&specforge_ops_registry::unusable_credential(diag)),
            }
        }
        (None, None) => unreachable!("exactly one source was checked"),
    };

    // An unreadable store is refused before any request, as publish refuses it (ADR 0045).
    let mut store = match CredentialStore::read(user.dir()) {
        Ok(store) => store,
        Err(diag) => return refuse(&specforge_ops_registry::unusable_credential(diag)),
    };

    let client = Retrying::new(HttpRegistryClient::new());
    let expires_at = match client.authenticate(&registry, &credential) {
        Ok(expires_at) => expires_at,
        Err(error) => return refuse(&specforge_ops_registry::failure(error)),
    };

    // Store credentials: a secret goes into a 0600 file and the store keeps
    // only metadata; a reference is kept as it is. Any legacy plaintext entry for this root is
    // replaced by this write.
    match reference {
        Some(reference) => store.keep_reference(root.clone(), reference),
        None => {
            if let Err(message) =
                store.keep_secret(root.clone(), credential.token().to_string(), expires_at)
            {
                return refuse(&OpError::coded(
                    specforge_ops::OpErrorKind::PreconditionFailed,
                    codes::R_LOGIN_002,
                    message,
                ));
            }
        }
    }

    if let Err(diag) = store.write() {
        return refuse(&OpError::of(specforge_ops::OpErrorKind::Internal, diag));
    }

    let kept = match store.registries.get(&root) {
        Some(CredentialEntry::EnvVar { .. }) => "env",
        Some(CredentialEntry::File { .. }) => "token_file",
        _ => "file",
    };
    match format {
        OutputFormat::Json => {
            let output = json!({
                "action": "login",
                "registry": alias,
                "root": root.as_str(),
                "status": "authenticated",
                "source": kept,
            });
            terminal.json(&output);
        }
        OutputFormat::Human => {
            let from = match (source.token_env, source.token_file) {
                (Some(var), _) => format!(" (token from ${var})"),
                (_, Some(file)) => format!(" (token from file {})", file.display()),
                _ => String::new(),
            };
            outln!(terminal, "logged in to registry '{alias}'{from}");
        }
    }

    Exit::Passed
}

/// Forget the credential kept for `url`, reading no project; else for the registry `registry_alias`
/// names in the project `project` names (the default registry when none is given).
fn logout(
    registry_alias: Option<&str>,
    url: Option<RegistryRoot>,
    project: &ProjectArg,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let refuse = |error: &OpError| Refusal::of(format).report(terminal, error);
    let (root, alias) = match url {
        Some(root) => (root, None),
        None => match project
            .root()
            .and_then(|root| specforge_ops_registry::configured(&root, "logout"))
        {
            Ok(configured) => {
                Notes::new(terminal, format).beside(&configured.diagnostics);
                match configured.named(registry_alias) {
                    Ok(registry) => (registry.url.clone(), Some(registry.alias.clone())),
                    Err(error) => return refuse(&error),
                }
            }
            Err(error) => return refuse(&error),
        },
    };
    let label = alias.clone().unwrap_or_else(|| root.to_string());
    let document = |status: &str| {
        let mut output = json!({ "action": "logout", "root": root.as_str(), "status": status });
        if let Some(alias) = &alias {
            output["registry"] = json!(alias);
        }
        output
    };

    let mut store = match CredentialStore::read(User::current().dir()) {
        Ok(store) => store,
        Err(diag) => return refuse(&specforge_ops_registry::unusable_credential(diag)),
    };
    let removed = store.forget(&root);

    if !removed {
        store.forget_secret(&root);
        match format {
            OutputFormat::Json => terminal.json(&document("not_found")),
            OutputFormat::Human => outln!(terminal, "no credentials found for registry '{label}'"),
        }
        return Exit::Passed;
    }

    // The entry first: a store that can't be written keeps its secret.
    if let Err(diag) = store.write() {
        return refuse(&OpError::of(specforge_ops::OpErrorKind::Internal, diag));
    }
    store.forget_secret(&root);

    match format {
        OutputFormat::Json => terminal.json(&document("removed")),
        OutputFormat::Human => outln!(terminal, "logged out from registry '{label}'"),
    }

    Exit::Passed
}
