#[cfg(test)]
mod tests {
    use archdrop::session::SessionStore;

    #[tokio::test]
    async fn test_create_session() {
        let store = SessionStore::new();

        // Create session
        let token = store.create_session("test.pdf".to_string()).await;
        assert!(!token.is_empty());

        // First validation passes
        let result = store.validate_and_mark_used(&token).await;
        assert_eq!(result, Some("test.pdf".to_string()));

        // Second validation fails
        let result = store.validate_and_mark_used(&token).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_invalid_token() {
        let store = SessionStore::new();

        // Validate random token should fail
        let result = store.validate_and_mark_used("bad token").await;
        assert_eq!(result, None);
    }
}
