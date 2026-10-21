use super::{ApiError, AuthToken, read_target};
use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use specforge_package::PackageName;
use specforge_registry_wire::{
    ErrorCode, PackageSignature, PublishReceipt, SignedContent, Yanked, form,
};
use std::sync::Arc;

use crate::db::{PackageVersion, PublishRefused};
use crate::publish::publish_target;
use crate::state::AppState;
use crate::stored::PublishFailed;

pub(super) async fn publish_package(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
    token: AuthToken,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<PublishReceipt>), ApiError> {
    let token_record = token.record;
    let (package, parsed) = publish_target(&name, &version)?;
    let (name, version) = (package.to_string(), parsed.to_string());

    // Rate limit: per token and per client IP (fixed window). Retry-After
    // rides both on the JSON body and the response header.
    if let Err(limited) = check_publish_rate(&state, &token_record, &headers) {
        return Err(ApiError::rate_limited(limited.retry_after));
    }

    ownership_precheck(&state, &token_record, &package).await?;

    // Check if version already exists
    let duplicate = {
        let (st, package, version) = (state.clone(), package.clone(), parsed.clone());
        tokio::task::spawn_blocking(move || st.stored().row(&package, &version).is_some())
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

    // An upload past the body limit is too large, not unsigned or badly signed.
    let too_large = |e: axum::extract::multipart::MultipartError| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::payload_too_large("the upload is larger than this registry accepts")
        } else {
            ApiError::bad_request(ErrorCode::BadRequest, e.to_string())
        }
    };
    while let Some(field) = multipart.next_field().await.map_err(too_large)? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            form::WASM => {
                wasm_bytes = Some(field.bytes().await.map_err(too_large)?.to_vec());
            }
            form::MANIFEST => {
                declaration_json = Some(field.text().await.map_err(too_large)?);
            }
            form::SIGNATURE => {
                signature_json = Some(field.text().await.map_err(too_large)?);
            }
            _ => {}
        }
    }

    // The publisher account recorded with the version: the scope owner's, which for the one
    // publishing into a scope it owns (or claims with this publish) is its own.
    let publisher_account = token_record.account.to_string();

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

    // C8-06 atomic publish: stage the blob, let the database's UNIQUE(name, version) arbitrate
    // concurrent publishes, then commit the blob (`Stored::publish`).
    let staged = {
        let (st, package, parsed, data) = (
            state.clone(),
            package.clone(),
            parsed.clone(),
            wasm_data.clone(),
        );
        tokio::task::spawn_blocking(move || st.stored().stage(&package, &parsed, &data))
            .await
            .expect("storage write task panicked")
            .map_err(|e| ApiError::internal(ErrorCode::StorageError, e))?
    };

    // The verified signature is stored so clients can verify offline again
    // (spec #21: the registry is not the trust anchor).
    let key_id = signature.key_id();
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

    let published = {
        let (st, row, package, writer) = (
            state.clone(),
            pkg.clone(),
            package.clone(),
            token_record.clone(),
        );
        tokio::task::spawn_blocking(move || st.stored().publish(staged, &row, &package, &writer))
            .await
            .expect("database insert task panicked")
    };
    match published {
        Ok(()) => {}
        Err(PublishFailed::Refused(PublishRefused::Write(refused))) => return Err(refused.into()),
        Err(PublishFailed::Refused(PublishRefused::DuplicateVersion(why))) => {
            return Err(ApiError::conflict(ErrorCode::DuplicateVersion, why));
        }
        Err(PublishFailed::Refused(PublishRefused::Storage(why))) => {
            return Err(ApiError::internal(ErrorCode::DbError, why));
        }
        Err(PublishFailed::Blob(why)) => {
            return Err(ApiError::internal(ErrorCode::StorageError, why));
        }
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

pub(super) async fn yank_package(
    State(state): State<Arc<AppState>>,
    Path((name, version)): Path<(String, String)>,
    token: AuthToken,
    headers: HeaderMap,
) -> Result<Json<Yanked>, ApiError> {
    let token_record = token.record;
    let (package, parsed) = read_target(&name, &version)?;

    // Rate limit: per token and per client IP (fixed window). Retry-After
    // rides both on the JSON body and the response header.
    if let Err(limited) = check_publish_rate(&state, &token_record, &headers) {
        return Err(ApiError::rate_limited(limited.retry_after));
    }

    let yanked = {
        let (st, package, parsed, writer) = (
            state.clone(),
            package.clone(),
            parsed.clone(),
            token_record.clone(),
        );
        tokio::task::spawn_blocking(move || st.stored().yank(&package, &parsed, &writer))
            .await
            .expect("yank write panicked")?
    };
    if yanked {
        tracing::info!("yanked {package}@{parsed}");
        Ok(Json(Yanked { yanked: true }))
    } else {
        Err(ApiError::not_found(format!("{package}@{parsed} not found")))
    }
}

/// The read-only ownership check, so a stranger's upload to an owned scope is refused before its body
/// is read; the publish transaction asks again (`Database::publish`).
async fn ownership_precheck(
    state: &Arc<AppState>,
    writer: &crate::db::TokenRecord,
    package: &PackageName,
) -> Result<(), ApiError> {
    let (st, writer, package) = (state.clone(), writer.clone(), package.clone());
    tokio::task::spawn_blocking(move || st.database.may_write(&writer, &package))
        .await
        .expect("ownership check panicked")?;
    Ok(())
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
