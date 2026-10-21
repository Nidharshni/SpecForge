//! `specforge login` and `specforge logout`: a registry credential, kept
//! under the root of the registry it is for (ADR 0178).

use crate::notes::Notes;
use crate::outcome::{Exit, OutputArg, OutputFormat, Refusal};
use crate::project::ProjectArg;
use crate::terminal::{Terminal, outln};
use serde_json::json;
use specforge_common::codes;
use specforge_ops::OpError;
use specforge_ops_registry::{
    ConfiguredRegistry, Kept, LoggedOut, RegistryRoot, TokenSource, User,
};
use std::path::PathBuf;

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
            Given {
                token: self.token,
                token_env: self.token_env,
                token_file: self.token_file,
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

/// The token flags as given: exactly one is allowed.
struct Given {
    token: Option<String>,
    token_env: Option<String>,
    token_file: Option<PathBuf>,
}

/// Validate the token `given` carries against the registry `registry_alias` names (the default registry
/// when none is given) and keep it for that registry's root: `--token` as a secret, `--token-env` and
/// `--token-file` as references.
fn login(
    registry_alias: Option<&str>,
    given: Given,
    project: &ProjectArg,
    format: OutputFormat,
    terminal: &Terminal,
) -> Exit {
    let refuse = |error: &OpError| Refusal::of(format).report(terminal, error);
    let source = match (given.token, given.token_env, given.token_file) {
        (Some(token), None, None) => TokenSource::Given(token),
        (None, Some(var), None) => TokenSource::Env(var),
        (None, None, Some(file)) => TokenSource::File(file),
        _ => {
            return refuse(&OpError::coded(
                specforge_ops::OpErrorKind::InvalidInput,
                codes::R_LOGIN_001,
                "login takes exactly one token source: --token <TOKEN>, --token-env <VAR> or --token-file <PATH>",
            ));
        }
    };
    let from = match &source {
        TokenSource::Env(var) => format!(" (token from ${var})"),
        TokenSource::File(file) => format!(" (token from file {})", file.display()),
        TokenSource::Given(_) => String::new(),
    };

    // The token is validated against a configured registry; with none,
    // fail before any network call (ADR 0004 N1).
    let root = match project.root() {
        Ok(root) => root,
        Err(error) => return refuse(&error),
    };
    let registry = ConfiguredRegistry::for_project(&root, "login");
    let result = registry.log_in(registry_alias, source);
    Notes::new(terminal, format).beside(&registry.reported());
    let logged_in = match result {
        Ok(logged_in) => logged_in,
        Err(error) => return refuse(&error),
    };

    match format {
        OutputFormat::Json => {
            terminal.json(&json!({
                "action": "login",
                "registry": logged_in.alias,
                "root": logged_in.root.as_str(),
                "status": "authenticated",
                "source": match logged_in.kept {
                    Kept::Secret => "file",
                    Kept::Env => "env",
                    Kept::File => "token_file",
                },
            }));
        }
        OutputFormat::Human => {
            outln!(
                terminal,
                "logged in to registry '{}'{from}",
                logged_in.alias
            );
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
    let logged_out = match url {
        Some(root) => User::current().log_out(&root),
        None => {
            let root = match project.root() {
                Ok(root) => root,
                Err(error) => return refuse(&error),
            };
            let registry = ConfiguredRegistry::for_project(&root, "logout");
            let result = registry.log_out(registry_alias);
            Notes::new(terminal, format).beside(&registry.reported());
            result
        }
    };
    let LoggedOut {
        alias,
        root,
        removed,
    } = match logged_out {
        Ok(logged_out) => logged_out,
        Err(error) => return refuse(&error),
    };
    let label = alias.clone().unwrap_or_else(|| root.to_string());

    match format {
        OutputFormat::Json => {
            let mut document = json!({
                "action": "logout",
                "root": root.as_str(),
                "status": if removed { "removed" } else { "not_found" },
            });
            if let Some(alias) = &alias {
                document["registry"] = json!(alias);
            }
            terminal.json(&document);
        }
        OutputFormat::Human if removed => outln!(terminal, "logged out from registry '{label}'"),
        OutputFormat::Human => outln!(terminal, "no credentials found for registry '{label}'"),
    }

    Exit::Passed
}
