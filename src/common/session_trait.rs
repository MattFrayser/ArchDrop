use crate::crypto::EncryptionKey;
use aes_gcm::Aes256Gcm;
use std::sync::Arc;

/// Core session functionality shared by all session types.
///
/// Provides authentication, encryption, and lifecycle management
/// for file transfer sessions.
pub trait Session {
    /// Returns the session token used for authentication
    fn token(&self) -> &str;

    /// Returns the encryption key for this session
    fn session_key(&self) -> &EncryptionKey;

    /// Returns the AES-GCM cipher instance for encryption/decryption
    fn cipher(&self) -> &Arc<Aes256Gcm>;

    /// Returns the session key encoded as base64
    fn session_key_b64(&self) -> String;

    /// Claims or validates the session for a specific client
    fn claim(&self, token: &str, client_id: &str) -> bool;

    /// Checks if the session is active for the given token and client
    fn is_active(&self, token: &str, client_id: &str) -> bool;

    /// Marks the session as complete for the given client
    fn complete(&self, token: &str, client_id: &str) -> bool;
}
