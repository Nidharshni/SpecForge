use super::{AdminToken, ApiError};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use sha2::{Digest, Sha256};
use specforge_registry_wire::{
    ErrorCode, TokenIssued, TokenList, TokenRequest, TokenRevoked, TokenSummary,
};
use std::sync::Arc;

use crate::auth;
use crate::state::AppState;

// --- Admin API (spec #21, T3): token lifecycle behind an admin-scoped bearer ---

pub(super) async fn admin_create_token(
    State(state): State<Arc<AppState>>,
    _admin: AdminToken,
    body: Option<Json<TokenRequest>>,
) -> (StatusCode, Json<TokenIssued>) {
    let Json(req) = body.unwrap_or_default();
    let expires_in_days = req.expires_in_days.unwrap_or(90);
    let created = {
        let st = state.clone();
        tokio::task::spawn_blocking(move || {
            let raw = auth::create_token(
                &st.database,
                req.scope.as_deref(),
                req.label.as_deref().unwrap_or("default"),
                Some(expires_in_days),
                false,
            );
            // The revocable identifier is the hash prefix (tokens are stored hashed).
            let mut hasher = Sha256::new();
            hasher.update(raw.as_bytes());
            let prefix = hex::encode(hasher.finalize())[..8].to_string();
            (raw, prefix)
        })
        .await
        .expect("token creation panicked")
    };
    let (token, prefix) = created;
    (
        StatusCode::CREATED,
        Json(TokenIssued {
            token,
            prefix,
            expires_in_days,
        }),
    )
}

pub(super) async fn admin_list_tokens(
    State(state): State<Arc<AppState>>,
    _admin: AdminToken,
) -> Result<Json<TokenList>, ApiError> {
    let tokens: Vec<TokenSummary> = {
        let st = state.clone();
        tokio::task::spawn_blocking(move || auth::list_tokens(&st.database))
            .await
            .expect("token listing panicked")
            .map_err(|e| ApiError::internal(ErrorCode::DbError, e))?
            .into_iter()
            .map(|t| TokenSummary {
                prefix: t.token_hash[..8.min(t.token_hash.len())].to_string(),
                scope: t.scope,
                label: t.label,
                created_at: t.created_at,
                expires_at: t.expires_at,
                admin: t.admin,
            })
            .collect()
    };
    Ok(Json(TokenList { tokens }))
}

pub(super) async fn admin_revoke_token(
    State(state): State<Arc<AppState>>,
    _admin: AdminToken,
    Path(id): Path<String>,
) -> Result<Json<TokenRevoked>, ApiError> {
    let revoked = {
        let st = state.clone();
        tokio::task::spawn_blocking(move || auth::revoke_token(&st.database, &id))
            .await
            .expect("token revocation panicked")
    };
    match revoked {
        Ok(revoked) => Ok(Json(TokenRevoked { revoked })),
        Err(crate::db::RevokeRefused::NotAnId) => Err(ApiError::bad_request(
            ErrorCode::BadRequest,
            "a token id is the 8 hex digits the listing shows",
        )),
        Err(crate::db::RevokeRefused::Ambiguous) => Err(ApiError::conflict(
            ErrorCode::BadRequest,
            "ambiguous token id: two live tokens share it",
        )),
        Err(crate::db::RevokeRefused::Storage(why)) => {
            Err(ApiError::internal(ErrorCode::DbError, why))
        }
    }
}
