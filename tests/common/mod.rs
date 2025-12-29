use aes_gcm::{Aes256Gcm, KeyInit};
use archdrop::common::TransferConfig;
use archdrop::crypto::types::EncryptionKey;
use sha2::digest::generic_array::GenericArray;
use tempfile::TempDir;

pub const CHUNK_SIZE: usize = 10 * 1024 * 1024; // 10MB
pub const CLIENT_ID: &str = "test-client-123";

pub fn default_config() -> TransferConfig {
    TransferConfig {
        chunk_size: CHUNK_SIZE as u64,
        concurrency: 8,
    }
}

pub fn setup_temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

pub fn create_cipher(key: &EncryptionKey) -> Aes256Gcm {
    Aes256Gcm::new(GenericArray::from_slice(key.as_bytes()))
}
