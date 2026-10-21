use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
};
use specforge_registry_wire::{ErrorBody, ErrorCode};

/// Typed API failure (C14-05): one IntoResponse implementation replaces the
/// hand-built `(StatusCode, Json)` tuples that mutating handlers used to
/// copy-paste. The body is the wire crate's [`ErrorBody`], so the code is an
/// [`ErrorCode`].
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: ErrorCode,
    pub message: String,
    /// Seconds for the Retry-After response header (rate limiting).
    pub retry_after_secs: Option<u64>,
}

impl ApiError {
    pub fn bad_request(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn conflict(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::forbidden_code(ErrorCode::Forbidden, message)
    }

    /// FORBIDDEN with a specific machine code (e.g. SCOPE_OWNED).
    pub fn forbidden_code(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: ErrorCode::NotFound,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: ErrorCode::BadRequest,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: ErrorCode::Unauthorized,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn rate_limited(retry_after: std::time::Duration) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: ErrorCode::RateLimited,
            message: format!(
                "too many publish requests; retry after {} seconds",
                retry_after.as_secs()
            ),
            retry_after_secs: Some(retry_after.as_secs()),
        }
    }

    pub fn internal(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut response =
            (self.status, Json(ErrorBody::new(self.code, self.message))).into_response();
        if let Some(secs) = self.retry_after_secs
            && let Ok(value) = secs.to_string().parse()
        {
            response.headers_mut().insert("retry-after", value);
        }
        response
    }
}

impl From<crate::ownership::WriteRefused> for ApiError {
    fn from(refused: crate::ownership::WriteRefused) -> Self {
        use crate::ownership::WriteRefused;
        let message = refused.message();
        match refused {
            WriteRefused::Outside { .. } => Self::forbidden(message),
            WriteRefused::Owned { .. } => Self::forbidden_code(ErrorCode::ScopeOwned, message),
            WriteRefused::Storage(_) => Self::internal(ErrorCode::DbError, message),
        }
    }
}
