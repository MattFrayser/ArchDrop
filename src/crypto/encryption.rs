use crate::crypto::types::Nonce;
use aes_gcm::{aead::Aead, Aes256Gcm};
use anyhow::Result;
use sha2::digest::generic_array::GenericArray;

// Encrypt and decrypt using AES-256-GCM
// Same cipher is used throughout sessions life

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
