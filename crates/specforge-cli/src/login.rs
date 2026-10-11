//! `specforge login` and `specforge logout`: a registry credential, kept
//! under the alias of the registry it is for (ADR 0045).

use crate::terminal::{Terminal, outln};

use crate::outcome::{Exit, OutputFormat, Refusal};
use serde_json::json;
use specforge_common::codes;
use specforge_ops::OpError;
use specforge_registry_client::credentials::{
    CredentialEntry, CredentialStore, TokenReference, credentials_path, read_credentials,
    write_credentials,
};
use specforge_registry_client::{HttpRegistryClient, RegistryClient, RegistryCredential, Retrying};
use std::path::Path;

/// Where the token to log in with comes from: exactly one of the three is given.
pub(crate) struct TokenSource<'a> {
    pub token: Option<&'a str>,
    pub token_env: Option<&'a str>,
    pub token_file: Option<&'a Path>,
}

/// Validate the token `source` gives against the registry `registry_alias` names (the default
/// registry when none is given) and keep it under that registry's alias: `--token` as a secret
/// (OS keyring, else a 0600 file), `--token-env` and `--token-file` as references.
pub(crate) fn run(
    registry_alias: Option<&str>,
    source: &TokenSource<'_>,
    path: &Path,
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
        return refuse(&OpError::diagnostic(
            codes::R_LOGIN_001,
            "login takes exactly one token source: --token <TOKEN>, --token-env <VAR> or --token-file <PATH>",
        ));
    }

    // The token is validated against a configured registry; with none,
    // fail before any network call (ADR 0004 N1).
    let configured = match specforge_ops_registry::configured(path, "login") {
        Ok(configured) => configured,
        Err(error) => return refuse(&error),
    };
    terminal.diagnostics(&configured.diagnostics);
    let registry = match configured.named(registry_alias) {
        Ok(registry) => registry.clone(),
        Err(error) => return refuse(&error),
    };
    let alias = registry.alias.as_str();

    // Resolve the token before any request: a reference that can't be read is R010/R011.
    let reference = match (source.token_env, source.token_file) {
        (Some(var), _) => Some(TokenReference::Env(var.to_string())),
        (_, Some(file)) => Some(TokenReference::File(file.to_path_buf())),
        _ => None,
    };
    let credential = match (&reference, source.token) {
        (_, Some(token)) => RegistryCredential::new(alias, token),
        (Some(reference), None) => {
            let mut probe = CredentialStore::default();
            probe.set_reference(alias, reference.clone());
            match probe.credential(alias) {
                Ok(Some(credential)) => credential,
                Ok(None) => unreachable!("a reference was just set"),
                Err(diag) => return refuse(&OpError::from(diag)),
            }
        }
        (None, None) => unreachable!("exactly one source was checked"),
    };

    let client = Retrying::new(HttpRegistryClient::new());
    let expires_at = match client.authenticate(&registry, &credential) {
        Ok(expires_at) => expires_at,
        Err(error) => return refuse(&OpError::from(error.to_diagnostic())),
    };

    // Store credentials: a secret goes into the OS keyring (0600-file fallback) and the file keeps
    // only metadata; a reference is kept as it is. Any legacy plaintext entry for this alias is
    // replaced by this write.
    let cred_path = credentials_path();
    let mut store = read_credentials(&cred_path).unwrap_or_default();
    match reference {
        Some(reference) => store.set_reference(alias, reference),
        None => {
            if let Err(message) = store.set_token(alias, credential.token().to_string(), expires_at)
            {
                return refuse(&OpError::diagnostic(codes::R_LOGIN_002, message));
            }
        }
    }

    if let Err(diag) = write_credentials(&cred_path, &store) {
        return refuse(&OpError::from(diag));
    }

    let kept = match store.registries.get(alias) {
        Some(CredentialEntry::EnvVar { .. }) => "env",
        Some(CredentialEntry::File { .. }) => "token_file",
        Some(CredentialEntry::Token {
            in_keyring: true, ..
        }) => "keyring",
        _ => "file",
    };
    match format {
        OutputFormat::Json => {
            let output = json!({
                "action": "login",
                "registry": alias,
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

/// Forget the credential of the registry `registry_alias` names, without
/// reading any project; with none given, of the default registry of the
/// project at `path`.
pub(crate) fn run_logout(
    registry_alias: Option<&str>,
    path: &Path,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let refuse = |error: &OpError| Refusal::of(format).report(terminal, error);
    let alias = match registry_alias {
        Some(alias) => alias.to_string(),
        None => match specforge_ops_registry::configured(path, "logout") {
            Ok(configured) => {
                terminal.diagnostics(&configured.diagnostics);
                match configured.named(None) {
                    Ok(registry) => registry.alias.clone(),
                    Err(error) => return refuse(&error),
                }
            }
            Err(error) => return refuse(&error),
        },
    };
    let alias = alias.as_str();
    let cred_path = credentials_path();

    let mut store = read_credentials(&cred_path).unwrap_or_default();
    let removed = store.remove(alias);
    specforge_registry_client::secrets::delete_secret(alias);

    if !removed {
        match format {
            OutputFormat::Json => {
                let output = json!({
                    "action": "logout",
                    "registry": alias,
                    "status": "not_found",
                });
                terminal.json(&output);
            }
            OutputFormat::Human => outln!(terminal, "no credentials found for registry '{alias}'"),
        }
        return Exit::Passed;
    }

    if let Err(diag) = write_credentials(&cred_path, &store) {
        return refuse(&OpError::from(diag));
    }

    match format {
        OutputFormat::Json => {
            let output = json!({
                "action": "logout",
                "registry": alias,
                "status": "removed",
            });
            terminal.json(&output);
        }
        OutputFormat::Human => outln!(terminal, "logged out from registry '{alias}'"),
    }

    Exit::Passed
}
