use super::{ApiError, Reader, read_name};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use specforge_package::Version;
use specforge_registry_wire::address::Call;
use specforge_registry_wire::{
    ErrorCode, PackageMetadata, SearchHit, SearchQuery, SearchResults, VersionList,
};
use std::sync::Arc;

use crate::state::AppState;

pub(super) async fn get_package_versions(
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

pub(super) async fn get_package_version(
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

    // A path at the registry's root, prefix included: the route this server mounts for the download.
    let wasm_url = Call::Download(&package, &parsed).path();
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

pub(super) async fn download_package(
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

pub(super) async fn search_packages(
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
