//! AES-256-GCM encryption with positioned nonces for out-of-order chunk processing.
//!
//! - Each file has a random 8-byte nonce base
//! - Per-chunk nonce = base + chunk_index (4-byte big-endian counter)
//! - Client derives same nonce from chunk position (no transmission overhead)
//!

use crate::crypto::types::Nonce;
use aes_gcm::{aead::Aead, Aes256Gcm};
use anyhow::Result;
use sha2::digest::generic_array::GenericArray;

pub fn decrypt_chunk_at_position(
    cipher: &Aes256Gcm,
    nonce_base: &Nonce,
    encrypted_data: &[u8],
    counter: u32,
) -> Result<Vec<u8>> {
    let full_nonce = nonce_base.with_counter(counter);
    let nonce_array = GenericArray::from_slice(&full_nonce);

    cipher
        .decrypt(nonce_array, encrypted_data)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))
}

pub fn encrypt_chunk_at_position(
    cipher: &Aes256Gcm,
    nonce_base: &Nonce,
    plaintext: &[u8],
    counter: u32,
) -> Result<Vec<u8>> {
    let full_nonce = nonce_base.with_counter(counter);
    let nonce_array = GenericArray::from_slice(&full_nonce);

    cipher
        .encrypt(nonce_array, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))
}
