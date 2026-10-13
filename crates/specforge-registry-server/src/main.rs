use specforge_registry_server::state::{AppState, PublishLimits, ReadAccess};
use specforge_registry_server::{auth, db::Database, handlers};

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "specforge-registry",
    version,
    about = "SpecForge extension registry server"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the registry server
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "4873")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Data directory for storage and database
        #[arg(long, default_value = "./registry-data")]
        data_dir: PathBuf,

        /// Require a valid token for every read (versions, metadata, download, search)
        #[arg(long, default_value_t = false)]
        private: bool,
    },
    /// Create an authentication token
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },
}

#[derive(Subcommand)]
enum TokenAction {
    /// Create a new token
    Create {
        /// Scope filter (e.g., "@myorg") — empty means full access
        #[arg(long)]
        scope: Option<String>,

        /// Label for the token
        #[arg(long, default_value = "default")]
        label: String,

        /// Days until the token expires (default policy: 90)
        #[arg(long, default_value_t = 90)]
        expires_in_days: u64,

        /// Token never expires
        #[arg(long, default_value_t = false)]
        no_expiry: bool,

        /// Grant token-administration rights (required to use the admin API)
        #[arg(long, default_value_t = false)]
        admin: bool,

        /// Data directory (must match the server's)
        #[arg(long, default_value = "./registry-data")]
        data_dir: PathBuf,
    },
    /// List all tokens
    List {
        /// Data directory
        #[arg(long, default_value = "./registry-data")]
        data_dir: PathBuf,
    },
    /// Revoke a token
    Revoke {
        /// Token id (the 8 hex digits `token list` shows)
        id: String,

        /// Data directory
        #[arg(long, default_value = "./registry-data")]
        data_dir: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "specforge_registry_server=info,tower_http=info"
                    .parse()
                    .unwrap()
            }),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            port,
            host,
            data_dir,
            private,
        } => {
            let read_access = if private {
                ReadAccess::Token
            } else {
                ReadAccess::Public
            };
            let app_state = Arc::new(
                AppState::open(&data_dir, PublishLimits::SERVE, read_access)
                    .expect("failed to open registry"),
            );

            let app = handlers::router(app_state);

            let addr = format!("{}:{}", host, port);
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .unwrap_or_else(|e| panic!("failed to bind to {}: {}", addr, e));

            tracing::info!(
                "registry server listening on http://{addr} (a registries entry's \"url\": \"http://{addr}\")"
            );

            axum::serve(listener, app).await.expect("server error");
        }
        Commands::Token { action } => match action {
            TokenAction::Create {
                scope,
                label,
                expires_in_days,
                no_expiry,
                admin,
                data_dir,
            } => {
                std::fs::create_dir_all(&data_dir).expect("failed to create data directory");
                let database =
                    Database::open(&data_dir.join("registry.db")).expect("failed to open database");

                let expires_in_days = if no_expiry {
                    None
                } else {
                    Some(expires_in_days)
                };
                let token =
                    auth::create_token(&database, scope.as_deref(), &label, expires_in_days, admin);
                println!("Token created successfully.\n");
                println!("  token: {}", token);
                println!("  scope: {}", scope.as_deref().unwrap_or("(full access)"));
                println!("  label: {}", label);
                println!(
                    "  expires: {}",
                    match expires_in_days {
                        Some(days) => format!("in {} days", days),
                        None => "never".to_string(),
                    }
                );
                println!("  admin: {}", if admin { "yes" } else { "no" });
                println!("\nStore this token securely — it cannot be retrieved later.");
            }
            TokenAction::List { data_dir } => {
                let database =
                    Database::open(&data_dir.join("registry.db")).expect("failed to open database");
                let tokens = auth::list_tokens(&database).expect("failed to list tokens");
                if tokens.is_empty() {
                    println!("No tokens found.");
                } else {
                    println!("{:<12} {:<20} {:<16} CREATED", "ID", "LABEL", "SCOPE");
                    for t in &tokens {
                        println!(
                            "{:<12} {:<20} {:<16} {}",
                            &t.token_hash[..8],
                            t.label,
                            t.scope.as_deref().unwrap_or("(all)"),
                            t.created_at,
                        );
                    }
                }
            }
            TokenAction::Revoke { id, data_dir } => {
                let database =
                    Database::open(&data_dir.join("registry.db")).expect("failed to open database");
                match auth::revoke_token(&database, &id) {
                    Ok(true) => println!("Token revoked."),
                    Ok(false) => println!("No token found with id '{id}'."),
                    Err(refused) => {
                        eprintln!("Token not revoked: {refused:?}");
                        std::process::exit(1);
                    }
                }
            }
        },
    }
}
