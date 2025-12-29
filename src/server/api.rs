use super::runtime;
use crate::common::{Manifest, TransferConfig};
use crate::crypto::types::{EncryptionKey, Nonce};
use crate::receive::{ReceiveAppState, ReceiveSession};
use crate::send::{SendAppState, SendSession};
use crate::server::progress::ProgressTracker;
use crate::server::routes;
use anyhow::Result;
use axum::Router;
use std::path::PathBuf;
use tokio::sync::watch;

// Based off cli flags
pub enum ServerMode {
    Local,
    Tunnel,
}

// Server configuration
pub struct ServerInstance {
    pub app: axum::Router,
    pub display_name: String, // shown in tui
    pub progress_sender: watch::Sender<f64>,
}

impl ServerInstance {
    pub fn new(app: Router, display_name: String, progress_sender: watch::Sender<f64>) -> Self {
        Self {
            app,
            display_name,
            progress_sender,
        }
    }

    // Tui status bar
    pub fn progress_receiver(&self) -> watch::Receiver<f64> {
        self.progress_sender.subscribe()
    }
}

pub fn get_transfer_config(mode: &ServerMode) -> TransferConfig {
    match mode {
        ServerMode::Tunnel => TransferConfig::tunnel(),
        ServerMode::Local => TransferConfig::local(),
    }
}

//----------------
// SEND SERVER
//---------------
pub async fn start_send_server(manifest: Manifest, mode: ServerMode) -> Result<u16> {
    let session_key = EncryptionKey::new();
    let nonce = Nonce::new();
    let config = get_transfer_config(&mode);

    // TUI display
    let display_name = if manifest.files.len() == 1 {
        manifest.files[0].name.clone()
    } else {
        format!("{} files", manifest.files.len())
    };

    // Send specific session
    let total_chunks = manifest.total_chunks(config.chunk_size);
    let send_session = SendSession::new(manifest, session_key, total_chunks);
    let (progress_sender, _) = tokio::sync::watch::channel(0.0);
    let progress_tracker = ProgressTracker::new(total_chunks, progress_sender.clone());

    // Create typed state for router
    let send_state = SendAppState::new(send_session.clone(), progress_tracker.clone(), config);
    let app = routes::create_send_router(&send_state);

    let server = ServerInstance::new(app, display_name, progress_sender);

    // Call runtime functions directly with typed state
    match mode {
        ServerMode::Local => runtime::start_https(server, send_state, nonce).await,
        ServerMode::Tunnel => runtime::start_tunnel(server, send_state, nonce).await,
    }
}

//----------------
// RECEIVE SERVER
//----------------
pub async fn start_receive_server(destination: PathBuf, mode: ServerMode) -> Result<u16> {
    let session_key = EncryptionKey::new();
    let nonce = Nonce::new();
    let config = get_transfer_config(&mode);

    // TUI display name
    let display_name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".")
        .to_string();

    // Receive specific session
    // Start with 0, will be updated when manifest arrives from client
    let receive_session = ReceiveSession::new(destination, session_key);
    let (progress_sender, _) = tokio::sync::watch::channel(0.0);
    let progress_tracker = ProgressTracker::new(0, progress_sender.clone()); // 0 chunks initially

    // Create typed state for router
    let receive_state =
        ReceiveAppState::new(receive_session.clone(), progress_tracker.clone(), config);
    let app = routes::create_receive_router(&receive_state);

    let server = ServerInstance::new(app, display_name, progress_sender);

    // Call runtime functions directly with typed state
    match mode {
        ServerMode::Local => runtime::start_https(server, receive_state, nonce).await,
        ServerMode::Tunnel => runtime::start_tunnel(server, receive_state, nonce).await,
    }
}
