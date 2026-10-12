use axum::{
    Router,
    extract::{DefaultBodyLimit, FromRequestParts, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode, request::Parts},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
};
use sha2::{Digest, Sha256};
use specforge_package::PackageName;
use specforge_package::Version;
use specforge_registry_wire::path::route;
use specforge_registry_wire::{
    ErrorBody, ErrorCode, PackageMetadata, PackageSignature, PublishReceipt, SearchHit,
    SearchQuery, SearchResults, SignedContent, TokenIssued, TokenList, TokenRequest, TokenRevoked,
    TokenSummary, TokenVerified, VersionList, Yanked, form, path,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::auth;
use crate::db::PackageVersion;
use crate::publish::publish_target;
use crate::state::AppState;
use crate::storage::LocalStorage;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(route::PACKAGE, get(get_package_versions))
        .route(route::VERSION, get(get_package_version))
        .route(
            route::VERSION,
            put(publish_package).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(route::VERSION, delete(yank_package))
        .route(route::DOWNLOAD, get(download_package))
        .route(route::SEARCH, get(search_packages))
        .route(route::AUTH_VERIFY, post(verify_auth))
        .route(
            route::ADMIN_TOKENS,
            post(admin_create_token).get(admin_list_tokens),
        )
        .route(route::ADMIN_TOKEN, delete(admin_revoke_token))
        .route(route::HEALTH, get(health_check))
        .with_state(state)
}

// --- Handlers ---

async fn health_check() -> &'static str {
    "ok"
}

async fn get_package_versions(
    State(state): State<Arc<AppState>>,
    _reader: Reader,
    Path(name): Path<String>,
) -> Result<Json<VersionList>, ApiError> {
    let name = read_name(&name)?.to_string();
    let versions = {
        let st = state.clone();
        let name = name.clone();
        tokio::task::spawn_blocking(move || st.database.get_package_versions(&name))
            .await
            .expect("package versions query panicked")
    }
    .map_err(|e| ApiError::internal(ErrorCode::DbError, e))?;

    if versions.is_empty() {
        return Err(ApiError::not_found(format!("package '{name}' not found")));
    }

    Ok(Json(VersionList { name, versions }))
}

async fn get_package_version(
    State(state): State<Arc<AppState>>,
    _reader: Reader,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<PackageMetadata>, ApiError> {
    let package = read_name(&name)?;
    let name = package.to_string();
    // A version that is not one cannot have been published: it is not found.
    let parsed = Version::parse(&version)
        .map_err(|_| ApiError::not_found(format!("{name}@{version} not found")))?;

    let pkg = {
        let st = state.clone();
        let name = name.clone();
        let version = version.clone();
        tokio::task::spawn_blocking(move || st.database.get_package_version(&name, &version))
            .await
            .map_err(|e| {
                ApiError::internal(
                    ErrorCode::DbTask,
                    format!("package version query task failed: {e}"),
                )
            })?
    };
    let Some(pkg) = pkg else {
        return Err(ApiError::not_found(format!("{name}@{version} not found")));
    };

    // Relative to the API base: clients compose this with their configured
    // registry URL, which already carries the /v1 prefix.
    let wasm_url = path::download(&package, &parsed);
    let keywords: Vec<String> = if pkg.keywords.is_empty() {
        vec![]
    } else {
        pkg.keywords
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    Ok(Json(PackageMetadata {
        name: pkg.name,
        version: pkg.version,
        sha256: pkg.sha256,
        size_bytes: pkg.size_bytes,
        description: pkg.description,
        keywords,
        publisher: pkg.publisher,
        published_at: pkg.published_at,
        wasm_url,
        signature: pkg.signature,
        manifest: pkg.manifest,
    }))
}

async fn download_package(
    State(state): State<Arc<AppState>>,
    _reader: Reader,
    Path((name, version)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let name = read_name(&name)?.to_string();

    // Storage reads are blocking file I/O: run them on the blocking pool.
    let storage_state = Arc::clone(&state);
    let storage_name = name.clone();
    let storage_version = version.clone();
    let data = tokio::task::spawn_blocking(move || {
        storage_state
            .storage
            .read_wasm(&storage_name, &storage_version)
    })
    .await
    .map_err(|e| {
        ApiError::internal(
            ErrorCode::StorageTask,
            format!("storage read task failed: {e}"),
        )
    })?;

    // C8-09 hardening: serve only blobs whose bytes hash to the DB's
    // recorded sha256 — torn or corrupted files are never handed out.
    // The DB lookup and the (up to 64 MB) hash run on the blocking pool.
    if let Some(data) = &data {
        let (expected, actual) = {
            let st = state.clone();
            let name = name.clone();
            let version = version.clone();
            let data = data.clone();
            tokio::task::spawn_blocking(move || {
                let expected = st
                    .database
                    .get_package_version(&name, &version)
                    .map(|row| row.sha256)
                    .unwrap_or_default();
                let actual = {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    hex::encode(hasher.finalize())
                };
                (expected, actual)
            })
            .await
            .map_err(|e| {
                ApiError::internal(
                    ErrorCode::IntegrityTask,
                    format!("integrity check task failed: {e}"),
                )
            })?
        };
        if !expected.is_empty() && actual != expected {
            return Err(ApiError::internal(
                ErrorCode::IntegrityViolation,
                "stored blob does not match its recorded digest",
            ));
        }
    }

    match data {
        Some(data) => Ok((
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/wasm")],
            data,
        )
            .into_response()),
        None => Err(ApiError::not_found(format!(
            "binary not found for {name}@{version}"
        ))),
    }
}

async fn search_packages(
    State(state): State<Arc<AppState>>,
    _reader: Reader,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResults>, ApiError> {
    let contributes = match query.contributes.as_deref() {
        None => None,
        Some(name) => Some(
            specforge_protocol_types::DeclaredCategory::from_name(name).ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::BadRequest,
                    format!(
                        "'{name}' is not a declared category; one of: {}",
                        specforge_protocol_types::DECLARED_CATEGORIES.join(", ")
                    ),
                )
            })?,
        ),
    };
    // rusqlite queries are blocking: run the search on the blocking pool.
    let results = tokio::task::spawn_blocking(move || {
        state.database.search(&query.q, query.limit, contributes)
    })
    .await
    .expect("search query task panicked")
    .map_err(|e| ApiError::internal(ErrorCode::DbError, e))?;

    let results = results
        .into_iter()
        .map(|p| SearchHit {
            name: p.name,
            version: p.version,
            description: p.description,
        })
        .collect();

    Ok(Json(SearchResults { results }))
}

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

/// Publish/yank authorization: the token must cover the package's scope.
fn require_publish_scope(
    record: &crate::db::TokenRecord,
    name: &PackageName,
) -> Result<(), ApiError> {
    if auth::token_has_scope(record, name) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "token does not have publish permission for scope '{name}'"
        )))
    }
}

async fn publish_package(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
    token: AuthToken,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<PublishReceipt>), ApiError> {
    let token_record = token.record;
    let (package, version) = publish_target(&name, &version)?;
    let (name, version) = (package.to_string(), version.to_string());

    // Rate limit: per token and per client IP (fixed window). Retry-After
    // rides both on the JSON body and the response header.
    if let Err(limited) = check_publish_rate(&state, &token_record, &headers) {
        return Err(ApiError::rate_limited(limited.retry_after));
    }

    require_publish_scope(&token_record, &package)?;

    // Check if version already exists
    let duplicate = {
        let st = state.clone();
        let name = name.clone();
        let version = version.clone();
        tokio::task::spawn_blocking(move || {
            st.database.get_package_version(&name, &version).is_some()
        })
        .await
        .expect("duplicate check panicked")
    };
    if duplicate {
        return Err(ApiError::conflict(
            ErrorCode::DuplicateVersion,
            format!("version {version} already exists for {name}"),
        ));
    }

    // Parse multipart form
    let mut wasm_bytes: Option<Vec<u8>> = None;
    let mut declaration_json: Option<String> = None;
    let mut signature_json: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            form::WASM => {
                wasm_bytes = field.bytes().await.ok().map(|b| b.to_vec());
            }
            form::MANIFEST => {
                declaration_json = field.text().await.ok();
            }
            form::SIGNATURE => {
                signature_json = field.text().await.ok();
            }
            _ => {}
        }
    }

    // --- Namespace ownership (spec #21, T6): first claim wins ---
    let scope = package
        .scope()
        .expect("publish_target refuses unscoped names")
        .to_string();
    let account_id = account_id_for(&token_record.token_hash);
    let claimed_now = {
        let claim = {
            let st = state.clone();
            let scope = scope.clone();
            let token_hash = token_record.token_hash.clone();
            let account_id = account_id.clone();
            tokio::task::spawn_blocking(move || {
                st.database.claim_scope(&scope, &token_hash, &account_id)
            })
            .await
            .expect("scope claim panicked")
        };
        match claim {
            Ok(claimed) => claimed,
            Err(e) => return Err(ApiError::internal(ErrorCode::StorageError, e)),
        }
    };
    if !claimed_now
        && let Some((owner_hash, owner_account)) = {
            let st = state.clone();
            let scope = scope.clone();
            tokio::task::spawn_blocking(move || st.database.get_scope_owner(&scope))
                .await
                .expect("scope owner query panicked")
        }
        && owner_hash != token_record.token_hash
    {
        return Err(ApiError::forbidden_code(
            ErrorCode::ScopeOwned,
            format!(
                "scope '{scope}' is owned by publisher '{owner_account}' — only the owning publisher may publish into it"
            ),
        ));
    }
    let publisher_account = state
        .database
        .get_scope_owner(&scope)
        .map(|(_, account)| account)
        .unwrap_or_else(|| account_id.clone());

    // --- Publish validation contract (spec #21, T3) ---
    // 1. Signed packages only: the signature field must be present.
    let signature = match signature_json.as_deref().map(str::trim) {
        None | Some("") => {
            return Err(ApiError::bad_request(
                ErrorCode::UnsignedPackage,
                "packages must be signed: run `specforge publish` (which signs) instead of uploading raw artifacts",
            ));
        }
        Some(text) => PackageSignature::parse(text).map_err(|why| {
            ApiError::bad_request(
                ErrorCode::InvalidSignature,
                format!("{why} (publish with `specforge publish`, which signs every package)"),
            )
        })?,
    };

    // 2. wasm magic bytes: reject non-wasm payloads.
    let wasm_data = match wasm_bytes {
        Some(d) if !d.is_empty() => d,
        _ => {
            return Err(ApiError::bad_request(
                ErrorCode::BadRequest,
                "missing 'wasm' field in multipart body",
            ));
        }
    };
    if !wasm_data.starts_with(b"\0asm") {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidWasm,
            "the uploaded 'wasm' field does not look like a Wasm binary (missing magic bytes)",
        ));
    }

    // 3. The manifest is the package's declaration (ADR 0012): it must
    //    parse as one.
    if declaration_json
        .as_deref()
        .is_none_or(|m| m.trim().is_empty())
    {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidManifest,
            "missing 'manifest' field in multipart body",
        ));
    }
    let declaration: specforge_protocol_types::ExtensionDeclaration = match serde_json::from_str(
        declaration_json.as_deref().unwrap_or(""),
    ) {
        Ok(d) => d,
        Err(e) => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidManifest,
                format!(
                    "manifest is not an extension declaration: {e} (publish it with \
                         `specforge publish`, which uploads the declaration it reads from the binary)"
                ),
            ));
        }
    };

    // 4. Its identity must match the upload URL.
    if declaration.name() != name || declaration.version() != version {
        return Err(ApiError::bad_request(
            ErrorCode::NameMismatch,
            format!(
                "manifest identifies {}@{} but the upload path is {}@{}",
                declaration.name(),
                declaration.version(),
                name,
                version
            ),
        ));
    }

    // 5. The signature verifies over this upload: its name, version, binary
    //    and exact manifest. Hashing is CPU-bound, so it runs on the blocking
    //    pool; the binary is hashed once, for the signature and the row.
    let manifest_json = declaration_json.unwrap_or_default();
    let (content, verified) = {
        let (name, version, wasm, manifest, signature) = (
            name.clone(),
            version.clone(),
            wasm_data.clone(),
            manifest_json.clone(),
            signature.clone(),
        );
        tokio::task::spawn_blocking(move || {
            let content = SignedContent::of(&name, &version, &wasm, &manifest);
            let verified = signature.verify(&content);
            (content, verified)
        })
        .await
        .expect("signature check task panicked")
    };
    verified.map_err(|why| {
        ApiError::bad_request(
            ErrorCode::InvalidSignature,
            format!("{why}: {name}@{version} as uploaded"),
        )
    })?;
    let sha256 = content.wasm_sha256().to_string();

    // --- end validation contract ---

    // What search shows: the description and keywords the declaration's
    // handshake carries.
    let description = declaration
        .handshake
        .description
        .clone()
        .unwrap_or_default();
    let keywords = declaration.handshake.keywords.join(",");

    // C8-06 atomic publish: (1) write the blob to a fsynced temp file,
    // (2) let the database's UNIQUE(name, version) arbitrate concurrent
    // publishes, (3) atomically rename the temp blob into place. Any
    // failure before step 3 leaves at most a collectable temp file —
    // never a torn or orphaned final blob.
    let store_state = Arc::clone(&state);
    let store_name = name.clone();
    let store_version = version.clone();
    let store_data = wasm_data.clone();
    let temp_path = match tokio::task::spawn_blocking(move || {
        store_state
            .storage
            .store_wasm_temp(&store_name, &store_version, &store_data)
    })
    .await
    .expect("storage write task panicked")
    {
        Ok(temp) => temp,
        Err(e) => return Err(ApiError::internal(ErrorCode::StorageError, e)),
    };

    // The verified signature is stored so clients can verify offline again
    // (spec #21: the registry is not the trust anchor).
    let key_id = signature.key_id();

    // Insert into database — rusqlite is blocking: run it on the blocking
    // pool.
    let pkg = PackageVersion {
        name: name.clone(),
        version: version.clone(),
        sha256,
        size_bytes: wasm_data.len() as u64,
        description,
        keywords,
        publisher: publisher_account,
        published_at: chrono::Utc::now().to_rfc3339(),
        signature: Some(signature),
        manifest: manifest_json,
    };

    // Run the insert on the blocking pool, but keep the temp path
    // available so a lost race (UNIQUE violation) can discard its blob.
    let insert_state = Arc::clone(&state);
    let insert_pkg = pkg.clone();
    let discard_temp = |p: &PathBuf| LocalStorage::discard_temp(p);
    let insert_result = {
        let temp = temp_path.clone();
        tokio::task::spawn_blocking(move || {
            let result = insert_state.database.insert_package(&insert_pkg);
            if result.is_err() {
                discard_temp(&temp);
            }
            result
        })
        .await
        .expect("database insert task panicked")
    };
    if let Err(e) = insert_result {
        return Err(ApiError::conflict(ErrorCode::DuplicateVersion, e));
    }

    // 3. Atomic rename — the moment the blob becomes visible.
    let commit_state = Arc::clone(&state);
    let commit_name = name.clone();
    let commit_version = version.clone();
    let commit_temp = temp_path.clone();
    let committed = tokio::task::spawn_blocking(move || {
        commit_state
            .storage
            .commit_wasm(&commit_name, &commit_version, &commit_temp)
    })
    .await
    .expect("blob commit task panicked");
    if let Err(e) = committed {
        // Compensate: the DB row must not outlive a missing blob.
        let rollback_state = Arc::clone(&state);
        let rollback_name = name.clone();
        let rollback_version = version.clone();
        let _ = tokio::task::spawn_blocking(move || {
            rollback_state
                .database
                .delete_package(&rollback_name, &rollback_version)
        })
        .await;
        return Err(ApiError::internal(ErrorCode::StorageError, e));
    }

    tracing::info!("published {}@{} ({} bytes)", name, version, wasm_data.len());

    Ok((
        StatusCode::CREATED,
        Json(PublishReceipt {
            name,
            version,
            sha256: pkg.sha256,
            size_bytes: pkg.size_bytes,
            key_id,
        }),
    ))
}

async fn yank_package(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
    token: AuthToken,
    headers: HeaderMap,
) -> Result<Json<Yanked>, ApiError> {
    let token_record = token.record;
    let package = read_name(&name)?;
    let name = package.to_string();

    // Rate limit: per token and per client IP (fixed window). Retry-After
    // rides both on the JSON body and the response header.
    if let Err(limited) = check_publish_rate(&state, &token_record, &headers) {
        return Err(ApiError::rate_limited(limited.retry_after));
    }

    require_publish_scope(&token_record, &package)?;

    let yanked = {
        let st = state.clone();
        let name = name.clone();
        let version = version.clone();
        tokio::task::spawn_blocking(move || st.database.yank_version(&name, &version))
            .await
            .expect("yank write panicked")
    };
    if yanked {
        tracing::info!("yanked {}@{}", name, version);
        Ok(Json(Yanked { yanked: true }))
    } else {
        Err(ApiError::not_found(format!("{name}@{version} not found")))
    }
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

// --- Admin API (spec #21, T3): token lifecycle behind an admin-scoped bearer ---

async fn admin_create_token(
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

async fn admin_list_tokens(
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

async fn admin_revoke_token(
    State(state): State<Arc<AppState>>,
    _admin: AdminToken,
    Path(prefix): Path<String>,
) -> Json<TokenRevoked> {
    let revoked = {
        let st = state.clone();
        let prefix = prefix.clone();
        tokio::task::spawn_blocking(move || auth::revoke_token(&st.database, &prefix))
            .await
            .expect("token revocation panicked")
    };
    Json(TokenRevoked { revoked })
}

/// Registry-assigned publisher identity: deterministic per issuing token.
fn account_id_for(token_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_hash.as_bytes());
    format!("acct_{}", &hex::encode(hasher.finalize())[..8])
}

/// The package a read or a yank names, from its URL segment. A name that
/// is not one cannot have been published, so it is not found.
fn read_name(segment: &str) -> Result<PackageName, ApiError> {
    PackageName::from_url_segment(segment)
        .map_err(|_| ApiError::not_found(format!("package '{segment}' not found")))
}

/// Fixed-window publish rate limit, keyed per token and per client IP.
fn check_publish_rate(
    state: &AppState,
    token_record: &crate::db::TokenRecord,
    headers: &HeaderMap,
) -> Result<(), crate::rate::Limited> {
    let token_key = format!(
        "publish:token:{}",
        &token_record.token_hash[..token_record.token_hash.len().min(12)]
    );
    state
        .rate_limiter
        .check(&token_key, state.publish_limit_per_token)?;
    if let Some(ip) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        let ip_key = format!("publish:ip:{}", ip.split(',').next().unwrap_or("").trim());
        state
            .rate_limiter
            .check(&ip_key, state.publish_limit_per_ip)?;
    }
    Ok(())
}
