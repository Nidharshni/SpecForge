use std::path::Path;

use crate::db::Database;
use crate::rate::RateLimiter;
use crate::storage::LocalStorage;

/// Who may read a registry: list versions, read metadata, download, search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadAccess {
    /// Anyone (the default; `specforge-registry serve`).
    #[default]
    Public,
    /// A request with a valid token only (`specforge-registry serve --private`); any unrevoked,
    /// unexpired token reads every package, whatever its scope.
    Token,
}

pub struct AppState {
    /// Who may read.
    pub read_access: ReadAccess,
    pub database: Database,
    pub storage: LocalStorage,
    /// Fixed-window rate limiter shared by all keyed checks.
    pub rate_limiter: RateLimiter,
    /// Publish requests allowed per token per window.
    pub publish_limit_per_token: u32,
    /// Publish requests allowed per client IP per window.
    pub publish_limit_per_ip: u32,
}

/// How many publishes (and yanks) a window allows per token and per client IP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishLimits {
    pub per_token: u32,
    pub per_ip: u32,
    /// How long a window lasts.
    pub window: std::time::Duration,
}

impl PublishLimits {
    /// What `specforge-registry serve` enforces: 30 per token, 60 per IP, per 60 s window.
    pub const SERVE: PublishLimits = PublishLimits {
        per_token: 30,
        per_ip: 60,
        window: std::time::Duration::from_secs(60),
    };
}

impl AppState {
    /// A registry whose database (`registry.db`) and binaries (`packages/`) live under `data_dir`, created
    /// when missing.
    pub fn open(
        data_dir: &Path,
        limits: PublishLimits,
        read_access: ReadAccess,
    ) -> Result<AppState, String> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| format!("failed to create data directory: {e}"))?;
        let database = Database::open(&data_dir.join("registry.db"))?;
        Ok(AppState {
            read_access,
            database,
            storage: LocalStorage::new(data_dir.join("packages"))?,
            rate_limiter: RateLimiter::new(limits.window.as_secs().max(1)),
            publish_limit_per_token: limits.per_token,
            publish_limit_per_ip: limits.per_ip,
        })
    }
}
