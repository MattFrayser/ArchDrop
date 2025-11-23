use axum::{Router, routing::get};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use axum::body::Body;
use axum::response::Response;
use axum::http::StatusCode;
use axum::extract::{Path, State};
use axum::response::Html;
use futures::stream::{self,StreamExt};
use std::sync::Arc;

use archdrop::session::SessionStore;
use archdrop::crypto::Encryptor;

pub async fn start_server(file_path: PathBuf) -> Result<u16, Box<dyn std::error::Error>> {
    //  port left 0 for OS to choose
    let addr = SocketAddr::from(([0, 0, 0, 0], 0)); // listen on all interfaces
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let port = listener.local_addr()?.port();

    let sessions = SessionStore::new();
    let encryptor = Encryptor::new();
    
    let key = encryptor.get_key_base64();
    let nonce = encryptor.get_nonce_base64();

    let token = sessions.create_session(file_path.to_string_lossy().to_string()).await;

    println!("Token: {}", token);
    println!(
        "URL: http://127.0.0.1:{}/download/{}#key={}&nonce={}",
        port, token, key, nonce
    );

    let state = AppState {
        sessions, 
        encryptor: Arc::new(encryptor),
    };


    // Create axium router
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/download/:token", get(serve_page))
        .route("/download/:token/data", get(download_handler))
        .route("/app.js", get(serve_js))
        .with_state(state);


    // Start server
    axum::serve(listener, app).await?;

    Ok(port)
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: SessionStore,
    pub encryptor: Arc<Encryptor>,  // Arc = thread-safe shared ownership
}

async fn download_handler(
    Path(token): Path<String>, 
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {

    println!("Download req for token: {}", token);
    // validate token and get file path
    let file_path = state.sessions
        .validate_and_mark_used(&token)
        .await
        .ok_or(StatusCode::FORBIDDEN)?; // None -> 403

    println!("Original file: {}", file_path);

    // open file asynchronously to not block thread
    let mut file = File::open(&file_path).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; // Error -> 500


    let mut encryptor = state.encryptor.create_stream_encryptor();

    // Async Stream
    // Create sream form state machine 
    let stream = stream::unfold(
        (file, encryptor, [0u8; 4096]), // 4KB buffer initial
        |(mut file, mut enc, mut buf)| async move {
        
        // consume buffer
        match file.read(&mut buf).await {
            Ok(0) => None, // EOF

            Ok(n) => {
                let chunk = &buf[..n]; // bytes read

                println!("Original chunk (20 bytes): {:?}", &chunk[..20.min(n)]);

                // encrypt chunk
                let encrypted = enc.encrypt_next(chunk)
                    .ok()?; // convert res to Option, end steam on err

                println!("Encrypted chunk (20 bytes): {:?}", &encrypted[..20.min(encrypted.len())]);

                // Frame format for browser parsing
                let len = encrypted.len() as u32;
                let mut framed = len.to_be_bytes().to_vec(); // prefix len
                framed.extend_from_slice(&encrypted); // append encrypted data

                // return (stream item, state for next)
                // Ok wraps body for Body::from_stream
                Some((Ok::<_, std::io::Error>(framed), (file, enc, buf)))
            }

            Err(e) => {
                Some((Err(e), (file, enc, buf)))
            }
        }
    },
    );

    println!("Starting stream");
    // Convert Stream to HTTP res body
    // Axum pulls items from stream and sends to client as produced
    Ok(Response::new(Body::from_stream(stream)))
}

async fn serve_page(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Html<&'static str>, StatusCode> {

    if !state.sessions.is_valid(&token).await {
        println!("token invalid or used");
        return Err(StatusCode::FORBIDDEN);
    }

    // return embedded html to brower
    const HTML: &str = include_str!("../templates/download.html");
    Ok(Html(HTML))
}

const JS: &str = include_str!("../templates/app.js");

async fn serve_js() -> Response {
    Response::builder()
        .header("content-type", "application/javascript")
        .body(Body::from(JS))
        .unwrap()
}



