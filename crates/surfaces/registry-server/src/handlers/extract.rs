use super::ApiError;
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};
use specforge_registry_wire::ErrorCode;
use std::sync::Arc;

use crate::auth;
use crate::state::AppState;

/// Extractor validating the bearer token once per request, off the async
/// runtime (C14-05): publish/yank/verify no longer repeat the
/// header-extract + validate_bearer sequence, and a future handler cannot
/// forget the check.
pub struct AuthToken {
    pub record: crate::db::TokenRecord,
}

impl AuthToken {
    async fn extract(state: &Arc<AppState>, headers: &HeaderMap) -> Result<Self, ApiError> {
        let auth_header = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("missing Authorization header"))?
            .to_string();
        let st = Arc::clone(state);
        let record =
            tokio::task::spawn_blocking(move || auth::validate_bearer(&st.database, &auth_header))
                .await
                .map_err(|e| {
                    ApiError::internal(
                        ErrorCode::AuthTask,
                        format!("auth validation task failed: {e}"),
                    )
                })?
                .ok_or_else(|| ApiError::unauthorized("invalid or revoked token"))?;
        Ok(Self { record })
    }
}

impl FromRequestParts<Arc<AppState>> for AuthToken {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Self::extract(state, &parts.headers).await
    }
}

/// Extractor for the read routes: under [`crate::state::ReadAccess::Token`] the request must carry a
/// valid token (any scope), else 401; under `Public` it admits everyone.
pub struct Reader;

impl FromRequestParts<Arc<AppState>> for Reader {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        if state.read_access == crate::state::ReadAccess::Token {
            AuthToken::extract(state, &parts.headers)
                .await
                .map_err(|_| ApiError::unauthorized("this registry requires a token to read"))?;
        }
        Ok(Reader)
    }
}

/// Admin-only extractor: same bearer validation as `AuthToken`, plus the
/// admin-scope check. Built on `AuthToken::extract` rather than a
/// hand-rolled second header-parse + `validate_bearer` call, so the admin
/// routes share one auth-checking implementation with everything else.
pub struct AdminToken {
    pub record: crate::db::TokenRecord,
}

impl FromRequestParts<Arc<AppState>> for AdminToken {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = AuthToken::extract(state, &parts.headers).await?;
        if !auth::is_admin(&token.record) {
            return Err(ApiError::forbidden_code(
                ErrorCode::AdminRequired,
                "this endpoint requires an admin token",
            ));
        }
        Ok(Self {
            record: token.record,
        })
    }
}
