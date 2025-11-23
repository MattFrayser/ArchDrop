use axum::{Router, routing::get};
use tokio::signal;
use tokio::sync::watch;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use axum::body::Body;
use axum::response::Response;
use axum::http::StatusCode;
use axum::extract::{Path, State};
use axum::response::Html;
use futures::stream;
use std::sync::Arc;

use crate::session::SessionStore;
use crate::crypto::Encryptor;
use crate::qr;
use crate::tui::TransferUI;


pub async fn start_server(file_path: PathBuf) -> Result<u16, Box<dyn std::error::Error>> {

    let sessions = SessionStore::new();
    let encryptor = Encryptor::new();

    // encrypion values
    let key = encryptor.get_key_base64();
    let nonce = encryptor.get_nonce_base64();
    let token = sessions.create_session(file_path.to_string_lossy().to_string()).await;

    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

    let tls_config = generate_cert(&local_ip).await?;

    // Tui values
    let (progress_sender, progress_consumer) = watch::channel(0.0); // make progress channel
    let file_hash = "";
    let file_name = file_path.file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let state = AppState {
        sessions,
        encryptor: Arc::new(encryptor),
        progress_sender: Arc::new(tokio::sync::Mutex::new(progress_sender)),
    };

    // Create axium router
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/download/:token", get(serve_page))
        .route("/download/:token/data", get(download_handler))
        .route("/app.js", get(serve_js))
        .with_state(state);

    //  port left 0 for OS to choose
    let addr = SocketAddr::from(([0, 0, 0, 0], 0)); // listen on all interfaces

    // Create a TCP listener to get the actual port assigned by the OS
    let listener = std::net::TcpListener::bind(addr)?;
    let actual_addr = listener.local_addr()?;
    let port = actual_addr.port();
    drop(listener); // Close the listener so axum-server can bind to the same port

    let addr_with_port = SocketAddr::from(([0, 0, 0, 0], port));

    let handle = axum_server::Handle::new();
    let server_handle = handle.clone();

    // HTTPS for local
    let url = format!(
        "https://{}:{}/download/{}#key={}&nonce={}",
        local_ip,
        port,
        token,
        key,
        nonce
    );

    let qr_code = qr::generate_qr(&url);

    // Spawn ctrl-c handler
    tokio::spawn(async move {
        signal::ctrl_c().await.ok();
        server_handle.shutdown();
    });

    let server = axum_server::bind_rustls(addr_with_port, tls_config)
        .handle(handle)
        .serve(app.into_make_service());


    // Spawn TUI
    tokio::spawn(async move { 
        let mut ui = TransferUI::new(
            progress_consumer,
            file_name,
            file_hash.to_owned(),
            qr_code,
            url, 
        );

        if let Err(e) = ui.run().await {
            eprintln!("ui err: {}", e);
        }
    });

    server.await?;
    Ok(port)
}

fn get_local_ip() -> Option<String> {
    // Connect to external address to determine local ip
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

use axum_server::tls_rustls::RustlsConfig;
use rcgen::generate_simple_self_signed;

async fn generate_cert(ip: &str) -> Result<RustlsConfig, Box<dyn std::error::Error>> {
    let subject_alt_names = vec![
        ip.to_string(),
        "localhost".to_string(),
    ];

    let cert = generate_simple_self_signed(subject_alt_names)?;
    let cert_pem = cert.serialize_pem()?;
    let key_pem = cert.serialize_private_key_pem();

    tokio::fs::write("/tmp/archdrop-cert.pem", &cert_pem).await?;
    tokio::fs::write("/tmp/archdrop-key.pem", &key_pem).await?;

    Ok(RustlsConfig::from_pem_file("/tmp/archdrop-cert.pem", "/tmp/archdrop-key.pem").await?)  
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: SessionStore,
    pub encryptor: Arc<Encryptor>,  // Arc = thread-safe shared ownership
    pub progress_sender: Arc<tokio::sync::Mutex<watch::Sender<f64>>>,
}

async fn download_handler(
    Path(token): Path<String>, 
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {

    // validate token and get file path
    let file_path = state.sessions
        .validate_and_mark_used(&token)
        .await
        .ok_or_else(|| {
            println!("Token validation failed");
            StatusCode::FORBIDDEN
        })?;// None -> 403

    println!("Token validated and marked as used");
    println!("Original file: {}", file_path);

    // open file asynchronously to not block thread
    let file = File::open(&file_path).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; // Error -> 500

    let encryptor = state.encryptor.create_stream_encryptor();

    // clone progress for stream
    let progress_sender = state.progress_sender.clone();

    // file meta data for progress
    let file_metadata = tokio::fs::metadata(&file_path).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; // Error -> 500
    let total_size = file_metadata.len() as f64;
    let bytes_sent = 0u64;

    // Async Stream
    // Create sream form state machine 
    // 4KB buffer initial
    let stream = stream::unfold(
        (file, encryptor, [0u8; 4096], bytes_sent, total_size, progress_sender),
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
                    let encrypted = enc.encrypt_next(chunk)
                        .ok()?; // convert res to Option, end steam on err

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
                        (file, enc, buf, bytes_sent, total_size, progress_sender)
                    ))
                }

                Err(e) => {
                    Some((
                        Err(e), 
                        (file, enc, buf, bytes_sent, total_size, progress_sender)
                    ))
                }
            }
    },
    );

    println!("Starting stream");
    // Convert Stream to HTTP res body
    // Axum pulls items from stream and sends to client as produced
    Ok(Response::new(Body::from_stream(stream)))
}

async fn serve_page() -> Result<Html<&'static str>, StatusCode> {

    // return embedded html to brower
    const HTML: &str = include_str!("../templates/download.html");
    Ok(Html(HTML))
}

const JS: &str = include_str!("../templates/app.js");

async fn serve_js() -> Response {
    Response::builder()
        .header("content-type", "application/javascript; charset=utf-8")
        .body(Body::from(JS))
        .unwrap()
}



