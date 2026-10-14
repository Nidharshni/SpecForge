use super::{ApiError, AuthToken, read_name};
use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use sha2::{Digest, Sha256};
use specforge_package::PackageName;
use specforge_registry_wire::{
    ErrorCode, PackageSignature, PublishReceipt, SignedContent, Yanked, form,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::auth;
use crate::db::{PackageVersion, PublishRefused};
use crate::publish::publish_target;
use crate::state::AppState;
use crate::storage::LocalStorage;

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

pub(super) async fn publish_package(
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

    // --- Namespace ownership (spec #21, T6): first publish wins ---
    // A read-only check, so a stranger's upload to an owned scope is refused before its body is
    // read. The scope is claimed only by the publish that stores a package, in one transaction with
    // the version (`Database::publish`).
    {
        let scope = package
            .scope()
            .expect("publish_target refuses unscoped names")
            .to_string();
        let st = state.clone();
        let owner = tokio::task::spawn_blocking(move || st.database.get_scope_owner(&scope))
            .await
            .expect("scope owner query panicked");
        if let Some((owner_hash, owner_account)) = owner
            && owner_hash != token_record.token_hash
        {
            return Err(ApiError::forbidden_code(
                ErrorCode::ScopeOwned,
                format!(
                    "scope '{}' is owned by publisher '{owner_account}' — only the owning publisher may publish into it",
                    package.scope().unwrap_or_default()
                ),
            ));
        }
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

    // The publisher account recorded with the version: the scope owner's, which for the one
    // publishing into a scope it owns (or claims with this publish) is its own.
    let scope = package
        .scope()
        .expect("publish_target refuses unscoped names")
        .to_string();
    let account_id = account_id_for(&token_record.token_hash);
    let publisher_account = account_id.clone();

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

    // Claim the scope and record the version in one transaction on the blocking pool, keeping the
    // temp path so a refusal (a lost version race, a scope claimed meanwhile) discards its blob.
    let insert_state = Arc::clone(&state);
    let insert_pkg = pkg.clone();
    let (claim_scope, claim_token, claim_account) = (
        scope.clone(),
        token_record.token_hash.clone(),
        account_id.clone(),
    );
    let discard_temp = |p: &PathBuf| LocalStorage::discard_temp(p);
    let published = {
        let temp = temp_path.clone();
        tokio::task::spawn_blocking(move || {
            let result = insert_state.database.publish(
                &insert_pkg,
                &claim_scope,
                &claim_token,
                &claim_account,
            );
            if result.is_err() {
                discard_temp(&temp);
            }
            result
        })
        .await
        .expect("database insert task panicked")
    };
    let claimed = match published {
        Ok(claimed) => claimed,
        Err(PublishRefused::ScopeOwned { account }) => {
            return Err(ApiError::forbidden_code(
                ErrorCode::ScopeOwned,
                format!(
                    "scope '{scope}' is owned by publisher '{account}' — only the owning publisher may publish into it"
                ),
            ));
        }
        Err(PublishRefused::DuplicateVersion(why)) => {
            return Err(ApiError::conflict(ErrorCode::DuplicateVersion, why));
        }
        Err(PublishRefused::Storage(why)) => {
            return Err(ApiError::internal(ErrorCode::DbError, why));
        }
    };

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
        let rollback_scope = scope.clone();
        let _ = tokio::task::spawn_blocking(move || {
            rollback_state.database.withdraw(
                &rollback_name,
                &rollback_version,
                &rollback_scope,
                claimed,
            )
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

pub(super) async fn yank_package(
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

/// Registry-assigned publisher identity: deterministic per issuing token.
fn account_id_for(token_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_hash.as_bytes());
    format!("acct_{}", &hex::encode(hasher.finalize())[..8])
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
