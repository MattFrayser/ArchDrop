use crate::errors::AppError;
use crate::server::session::Session;

#[derive(serde::Deserialize)]
pub struct ClientIdParam {
    #[serde(rename = "clientId")]
    pub client_id: String,
}

// Used for handlers that should only work with already claimed sessions
pub fn require_active_session(
    session: &Session,
    token: &str,
    client_id: &str,
) -> Result<(), AppError> {
    // Reject empty client_id
    if client_id.trim().is_empty() {
        return Err(anyhow::anyhow!("Client ID cannot be empty").into());
    }

    if !session.is_active(token, client_id) {
        return Err(anyhow::anyhow!("Invalid or inactive session").into());
    }
    Ok(())
}

// Used for handlers that initiate transfer
pub fn claim_or_validate_session(
    session: &Session,
    token: &str,
    client_id: &str,
) -> Result<(), AppError> {
    // Reject empty client_id
    if client_id.trim().is_empty() {
        return Err(anyhow::anyhow!("Client ID cannot be empty").into());
    }

    if !session.claim(token, client_id) {
        return Err(
            anyhow::anyhow!("Invalid token or session already claimed by another client").into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::types::EncryptionKey;
    use std::path::PathBuf;

    #[test]
    fn test_require_active_unclaimed_session() {
        // Create an unclaimed session
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();

        // Attempt to require active session without claiming first
        let result = require_active_session(&session, token, "client1");

        // Should fail because session is unclaimed
        assert!(result.is_err());
    }

    #[test]
    fn test_require_active_wrong_client_id() {
        // Create session and claim with client A
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();
        assert!(session.claim(token, "client_a"));

        // Try to access with client B
        let result = require_active_session(&session, token, "client_b");

        // Should fail because client_id doesn't match
        assert!(result.is_err());
    }

    #[test]
    fn test_require_active_valid_session() {
        // Create session and claim with client A
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();
        assert!(session.claim(token, "client_a"));

        // Access with same client should succeed
        let result = require_active_session(&session, token, "client_a");
        assert!(result.is_ok());
    }

    #[test]
    fn test_claim_or_validate_idempotent() {
        // Create session
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();

        // First claim should succeed
        let result1 = claim_or_validate_session(&session, token, "client_a");
        assert!(result1.is_ok());

        // Same client claiming again should succeed (idempotent)
        let result2 = claim_or_validate_session(&session, token, "client_a");
        assert!(result2.is_ok());
    }

    #[test]
    fn test_claim_or_validate_different_client() {
        // Create session and claim with client A
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();

        // Client A claims successfully
        let result1 = claim_or_validate_session(&session, token, "client_a");
        assert!(result1.is_ok());

        // Client B tries to claim - should fail
        let result2 = claim_or_validate_session(&session, token, "client_b");
        assert!(result2.is_err());
    }

    #[test]
    fn test_invalid_token_format() {
        // Create session with valid token
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);

        // Try to claim with wrong token
        let result = claim_or_validate_session(&session, "invalid-token-12345", "client_a");

        // Should fail because token doesn't match
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_client_id() {
        // Create session
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();

        // Empty client_id should be rejected
        let result = claim_or_validate_session(&session, token, "");
        assert!(result.is_err(), "Empty client_id should be rejected");

        // Whitespace-only should also be rejected
        let result2 = claim_or_validate_session(&session, token, "   ");
        assert!(result2.is_err(), "Whitespace-only client_id should be rejected");
    }

    #[test]
    fn test_require_active_with_wrong_token() {
        // Create and claim session
        let key = EncryptionKey::new();
        let session = Session::new_receive(PathBuf::from("/tmp/test"), key, 0);
        let token = session.token();
        assert!(session.claim(token, "client_a"));

        // Try to access with wrong token
        let result = require_active_session(&session, "wrong-token", "client_a");
        assert!(result.is_err());
    }
}
