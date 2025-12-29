use std::sync::Arc;
use std::time::Duration;

use crate::common::{AppError, Manifest, Session};
use crate::crypto::{self, Nonce};
use crate::send::file_handle::SendFileHandle;
use crate::server::auth::{self, ClientIdParam};
use anyhow::{Context, Result};
use axum::extract::Query;
use axum::{
    body::Body,
    extract::{Path, State},
    http::Response,
    Json,
};
use reqwest::header;
use tokio::time::sleep;

use super::SendAppState;

#[derive(serde::Deserialize)]
pub struct ChunkParams {
    #[serde(rename = "clientId")]
    client_id: String,
}

// Client will use manifest to know what it is downloading
pub async fn manifest_handler(
    Path(token): Path<String>,
    Query(params): Query<ClientIdParam>,
    State(state): State<SendAppState>,
) -> Result<Json<Manifest>, AppError> {
    // Session claimed when fetching manifest
    // Manifests holds info about files (sizes, names) only client should see
    auth::claim_or_validate_session(&state.session, &token, &params.client_id)?;

    // Get manifest from session
    let manifest = state.session.manifest();

    Ok(Json(manifest.clone()))
}

pub async fn send_handler(
    Path((token, file_index, chunk_index)): Path<(String, usize, usize)>,
    Query(params): Query<ChunkParams>,
    State(state): State<SendAppState>,
) -> Result<Response<Body>, AppError> {
    // Sessions are claimed by manifest, so just check client
    let client_id = &params.client_id;
    auth::require_active_session(&state.session, &token, client_id)?;

    // Some browser send multiple retries (safari)
    // Be noted to not count towards total
    let is_retry = state.session.has_chunk_been_sent(file_index, chunk_index);
    if !is_retry {
        state.session.mark_chunk_sent(file_index, chunk_index);
        state.progress.increment();
    }

    let file_entry = state
        .session
        .get_file(file_index)
        .ok_or_else(|| AppError::BadRequest(format!("file_index out of bounds: {}", file_index)))?;
    let chunk_size = state.config.chunk_size;

    // Get or create file handle (lazy initialization)
    let file_handle = state
        .file_handles
        .entry(file_index)
        .or_try_insert_with(|| -> Result<Arc<SendFileHandle>> {
            Ok(Arc::new(SendFileHandle::open(
                file_entry.full_path.clone(),
                file_entry.size,
            )?))
        })?
        .value()
        .clone();

    let encrypted_chunk = process_chunk(
        &file_handle,
        chunk_index,
        state.session.cipher(),
        chunk_size,
        file_entry.size,
        &file_entry.nonce,
    )
    .await?;

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(encrypted_chunk))
        .context("build response")?)
}

async fn process_chunk(
    file_handle: &Arc<SendFileHandle>,
    chunk_index: usize,
    cipher: &Arc<aes_gcm::Aes256Gcm>,
    chunk_size: u64,
    file_size: u64,
    nonce_str: &str,
) -> Result<Vec<u8>> {
    let start = chunk_index as u64 * chunk_size;

    // Validate bounds
    if start >= file_size {
        return Err(anyhow::anyhow!(
            "Chunk start {} exceeds file size {}",
            start,
            file_size
        ));
    }

    let end = std::cmp::min(start + chunk_size, file_size);
    let chunk_len = (end - start) as usize;

    // Read from disk using persistent handle
    let file_handle = file_handle.clone();
    let buffer = tokio::task::spawn_blocking(move || file_handle.read_chunk(start, chunk_len))
        .await
        .context("File read task panicked")??;

    // Prepare data to move into the closure
    let cipher = cipher.clone();
    let nonce_str = nonce_str.to_string();

    // Offload encryption to a blocking thread
    // This prevents AES-GCM from stalling the async runtime
    tokio::task::spawn_blocking(move || {
        let file_nonce = Nonce::from_base64(&nonce_str)?;
        crypto::encrypt_chunk_at_position(&cipher, &file_nonce, &buffer, chunk_index as u32)
            .context("Encryption failed")
    })
    .await?
}

pub async fn complete_download(
    Path(token): Path<String>,
    Query(params): Query<ChunkParams>,
    State(state): State<SendAppState>,
) -> Result<axum::Json<serde_json::Value>, AppError> {

    // Session must be active and owned to complete
    let client_id = &params.client_id;

    // If the session is ALREADY completed, resend the success signal
    // and return 200 OK. Handles the client retrying on network failure.
    if state.session.complete(&token, client_id) {
        state.progress.complete();
        return Ok(axum::Json(serde_json::json!({
           "success": true,
           "message": "Already completed"
        })));
    }

    let chunks_sent = state.session.get_chunks_sent();
    let total_chunks = state.session.get_total_chunks();

    auth::require_active_session(&state.session, &token, client_id)?;

    // Verify all chunks were actually sent
    if chunks_sent < total_chunks {
        tracing::warn!(
            "Complete called prematurely: {}/{} chunks sent ({}% complete)",
            chunks_sent,
            total_chunks,
            (chunks_sent as f64 / total_chunks as f64 * 100.0)
        );
    }

    state.session.complete(&token, client_id);

    // preprepare body
    let response_body = axum::Json(serde_json::json!({
        "success": true,
        "message": "Download successful. Initiating server shutdown."
    }));

    // Wait until Axum response leaves to signal shutdown on 100%
    // 50ms should be enough to ensure proper HTTP res
    let progress_clone = state.progress.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(50)).await;
        eprintln!("TUI shutdown signal (100.0) sent successfully. Exiting now.");
        progress_clone.complete();
    });

    Ok(response_body)
}
