use crate::crypto::Encryptor;
use crate::session::SessionStore;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, Response};
use futures::stream;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::watch;

#[derive(Clone)]
pub struct AppState {
    pub sessions: SessionStore,
    pub encryptor: Arc<Encryptor>, // Arc = thread-safe shared ownership
    pub progress_sender: Arc<tokio::sync::Mutex<watch::Sender<f64>>>,
}

pub async fn download_handler(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    // validate token and get file path
    let file_path = state
        .sessions
        .validate_and_mark_used(&token)
        .await
        .ok_or_else(|| {
            println!("Token validation failed");
            StatusCode::FORBIDDEN
        })?; // None -> 403

    println!("Token validated and marked as used");
    println!("Original file: {}", file_path);

    // Extract filename
    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download"); // default to generic 'download'

    // open file asynchronously to not block thread
    let file = File::open(&file_path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; // Error -> 500

    let encryptor = state.encryptor.create_stream_encryptor();

    // clone progress for stream
    let progress_sender = state.progress_sender.clone();

    // file meta data for progress
    let file_metadata = tokio::fs::metadata(&file_path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; // Error -> 500
    let total_size = file_metadata.len() as f64;
    let bytes_sent = 0u64;

    // Async Stream
    // Create sream form state machine
    // 4KB buffer initial
    let stream = stream::unfold(
        (
            file,
            encryptor,
            [0u8; 4096],
            bytes_sent,
            total_size,
            progress_sender,
        ),
        |(mut file, mut enc, mut buf, mut bytes_sent, total_size, progress_sender)| async move {
            //consume buffer
            match file.read(&mut buf).await {
                Ok(0) => {
                    let _ = progress_sender.lock().await.send(100.0);
                    None
                }
                Ok(n) => {
                    let chunk = &buf[..n]; // bytes read

                    // encrypt chunk
                    let encrypted = enc.encrypt_next(chunk).ok()?; // convert res to Option, end steam on err

                    // Frame format for browser parsing
                    let len = encrypted.len() as u32;
                    let mut framed = len.to_be_bytes().to_vec(); // prefix len
                    framed.extend_from_slice(&encrypted); // append encrypted data

                    // update progress
                    bytes_sent += n as u64;
                    let progress = (bytes_sent as f64 / total_size) * 100.0;
                    let _ = progress_sender.lock().await.send(progress);

                    // return (stream item, state for next)
                    // Ok wraps body for Body::from_stream
                    Some((
                        Ok::<_, std::io::Error>(framed),
                        (file, enc, buf, bytes_sent, total_size, progress_sender),
                    ))
                }

                Err(e) => Some((
                    Err(e),
                    (file, enc, buf, bytes_sent, total_size, progress_sender),
                )),
            }
        },
    );

    println!("Starting stream");
    Response::builder()
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from_stream(stream))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn serve_page() -> Result<Html<&'static str>, StatusCode> {
    // return embedded html to brower
    const HTML: &str = include_str!("../../templates/download.html");
    Ok(Html(HTML))
}

pub async fn serve_js() -> Response {
    const JS: &str = include_str!("../../templates/app.js");
    Response::builder()
        .header("content-type", "application/javascript; charset=utf-8")
        .body(Body::from(JS))
        .unwrap()
}
