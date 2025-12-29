//! Router definitions for send and receive modes

use crate::{
    receive::{self, ReceiveAppState},
    send::{self, SendAppState},
    ui::web,
};
use axum::{extract::DefaultBodyLimit, routing::*, Router};

/// Create router for send mode
pub fn create_send_router(state: &SendAppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route(
            "/send/:token/manifest",
            get(send::handlers::manifest_handler),
        )
        .route(
            "/send/:token/:file_index/chunk/:chunk_index",
            get(send::handlers::send_handler),
        )
        .route(
            "/send/:token/complete",
            post(send::handlers::complete_download),
        )
        .route("/send/:token", get(web::serve_download_page))
        .route("/download.js", get(web::serve_download_js))
        .route("/styles.css", get(web::serve_shared_css))
        .route("/shared.js", get(web::serve_shared_js))
        .with_state(state.clone())
}

/// Create router for receive mode
pub fn create_receive_router(state: &ReceiveAppState) -> Router {
    // CHUNK_SIZE is 10MB (local mode), with encryption + FormData overhead ~10.5MB per request
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route(
            "/receive/:token/manifest",
            post(receive::handlers::receive_manifest),
        )
        .route(
            "/receive/:token/chunk",
            post(receive::handlers::receive_handler),
        )
        .route(
            "/receive/:token/finalize",
            post(receive::handlers::finalize_upload),
        )
        .route("/receive/:token", get(web::serve_upload_page))
        .route(
            "/receive/:token/complete",
            post(receive::handlers::complete_transfer),
        )
        .route("/upload.js", get(web::serve_upload_js))
        .route("/styles.css", get(web::serve_shared_css))
        .route("/shared.js", get(web::serve_shared_js))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(25 * 1024 * 1024))
}
