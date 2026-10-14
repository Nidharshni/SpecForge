use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    response::Json,
    routing::{delete, get, post, put},
};
use specforge_package::PackageName;
use specforge_registry_wire::TokenVerified;
use specforge_registry_wire::address::{Endpoint, HEALTH};
use std::sync::Arc;

use crate::state::AppState;

mod admin;
mod error;
mod extract;
mod read;
mod write;

pub use error::ApiError;
pub use extract::{AdminToken, AuthToken, Reader};

pub fn router(state: Arc<AppState>) -> Router {
    let version = Endpoint::Version.route();
    Router::new()
        .route(&Endpoint::Package.route(), get(read::get_package_versions))
        .route(&version, get(read::get_package_version))
        .route(
            &version,
            put(write::publish_package).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(&version, delete(write::yank_package))
        .route(&Endpoint::Download.route(), get(read::download_package))
        .route(&Endpoint::Search.route(), get(read::search_packages))
        .route(&Endpoint::AuthVerify.route(), post(verify_auth))
        .route(
            &Endpoint::AdminTokens.route(),
            post(admin::admin_create_token).get(admin::admin_list_tokens),
        )
        .route(
            &Endpoint::AdminToken.route(),
            delete(admin::admin_revoke_token),
        )
        .route(HEALTH, get(health_check))
        .with_state(state)
}

// --- Handlers ---

async fn health_check() -> &'static str {
    "ok"
}

async fn verify_auth(State(_state): State<Arc<AppState>>, token: AuthToken) -> Json<TokenVerified> {
    let record = token.record;
    Json(TokenVerified {
        valid: true,
        scope: record.scope,
        label: record.label,
        expires_at: record.expires_at,
    })
}

/// The package a read or a yank names, from its URL segment. A name that
/// is not one cannot have been published, so it is not found.
fn read_name(segment: &str) -> Result<PackageName, ApiError> {
    PackageName::from_url_segment(segment)
        .map_err(|_| ApiError::not_found(format!("package '{segment}' not found")))
}
