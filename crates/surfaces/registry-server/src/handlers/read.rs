use super::{ApiError, Reader, read_name, read_target};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
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
    let package = read_name(&name)?;
    let versions = {
        let (st, package) = (state.clone(), package.clone());
        tokio::task::spawn_blocking(move || st.stored().versions(&package))
            .await
            .expect("package versions query panicked")
    }
    .map_err(|e| ApiError::internal(ErrorCode::DbError, e))?;

    if versions.is_empty() {
        return Err(ApiError::not_found(format!(
            "package '{package}' not found"
        )));
    }

    Ok(Json(VersionList {
        name: package.to_string(),
        versions,
    }))
}

pub(super) async fn get_package_version(
    State(state): State<Arc<AppState>>,
    _reader: Reader,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<PackageMetadata>, ApiError> {
    let (package, parsed) = read_target(&name, &version)?;

    let pkg = {
        let (st, package, parsed) = (state.clone(), package.clone(), parsed.clone());
        tokio::task::spawn_blocking(move || st.stored().row(&package, &parsed))
            .await
            .map_err(|e| {
                ApiError::internal(
                    ErrorCode::DbTask,
                    format!("package version query task failed: {e}"),
                )
            })?
    };
    let Some(pkg) = pkg else {
        return Err(ApiError::not_found(format!("{package}@{parsed} not found")));
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
    let (package, parsed) = read_target(&name, &version)?;
    // Storage reads and the (up to 64 MB) hash are blocking work: run them on the blocking pool.
    let (st, p, v) = (state.clone(), package.clone(), parsed.clone());
    let data = tokio::task::spawn_blocking(move || st.stored().blob(&p, &v))
        .await
        .map_err(|e| {
            ApiError::internal(
                ErrorCode::StorageTask,
                format!("storage read task failed: {e}"),
            )
        })?
        .map_err(|_| {
            ApiError::internal(
                ErrorCode::IntegrityViolation,
                "stored blob does not match its recorded digest",
            )
        })?
        .ok_or_else(|| ApiError::not_found(format!("binary not found for {package}@{parsed}")))?;
    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/wasm")],
        data,
    )
        .into_response())
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
