use aes_gcm::{
    aead::{OsRng},
    aead::stream::{EncryptorBE32},  // ← Stream types
    aead::generic_array::GenericArray,  // ← For type conversions
    Aes256Gcm,
};
use rand::RngCore;
use sha2::{Sha256, Digest};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Encryptor {
    key: [u8; 32], 
    // EncryptorBE32 adds 32-bit counter + 8-bit last-block flag
    // 7 bytes nonce + 4 bytes counter + 1 byte flag = 12 bytes
    nonce: [u8; 7], 
}

impl Encryptor {
    pub fn new() -> Self {
        let mut key = [0u8; 32];
        let mut nonce = [0u8; 7];
        OsRng::default().fill_bytes(&mut key);
        OsRng::default().fill_bytes(&mut nonce);

        Self { key, nonce }
    }

    pub fn create_stream_encryptor(&self) -> EncryptorBE32<Aes256Gcm> {
        // Convert [u8] to GenericArray<u8, U32> for aes_gcm crate
        let key = GenericArray::from_slice(&self.key);
        let nonce = GenericArray::from_slice(&self.nonce);

        // EncryptorBE32 handles nonce increment automatically
        // Internally constructs: [7 random bytes][5 bytes for counter]
        EncryptorBE32::new(key, nonce)
    }
    pub fn get_key_base64(&self) -> String {
        base64::encode(&self.key)
    }

    pub fn get_nonce_base64(&self) -> String {
        base64::encode(&self.nonce)
    }
}

pub async fn calculate_file_hash(path: &str) -> Result<String, std::io::Error> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}




