pub mod handlers;
pub mod modes;
pub mod utils;

use crate::crypto::Encryptor;
use crate::server::handlers::AppState;
use crate::session::SessionStore;
use axum::{routing::get, Router};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

pub enum ServerMode {
    Local,
    Http,
    Tunnel,
}

pub async fn start_server(
    file_path: PathBuf,
    mode: ServerMode,
) -> Result<u16, Box<dyn std::error::Error>> {
    let sessions = SessionStore::new();
    let encryptor = Encryptor::new();

    // encrypion values
    let key = encryptor.get_key_base64();
    let nonce = encryptor.get_nonce_base64();
    let token = sessions
        .create_session(file_path.to_string_lossy().to_string())
        .await;

    // Progress channel
    let (progress_sender, progress_consumer) = watch::channel(0.0); // make progress channel
    let file_hash = "";
    let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();

    let state = AppState {
        sessions,
        encryptor: Arc::new(encryptor),
        progress_sender: Arc::new(tokio::sync::Mutex::new(progress_sender)),
    };

    // Create axium router
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/download/:token", get(handlers::serve_page))
        .route("/download/:token/data", get(handlers::download_handler))
        .route("/app.js", get(handlers::serve_js))
        .with_state(state);

    let server = modes::Server {
        app,
        token,
        key,
        nonce,
        progress_consumer,
        file_name,
        file_hash: file_hash.to_owned(),
    };

    match mode {
        ServerMode::Local => modes::start_local(server).await,
        ServerMode::Tunnel => modes::start_tunnel(server).await,
        ServerMode::Http => modes::start_http(server).await,
    }
}
