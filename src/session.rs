use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<Mutex<HashMap<String, SessionData>>>,
}

pub struct SessionData {
    pub file_path: String, 
    pub used: bool, // flag to prevent replay attacks
}

impl SessionStore {
    pub fn new() -> Self {
        Self { 
            // wrap hashmap in mutex for safe access
            sessions: Arc::new(Mutex::new(HashMap::new())), 
        }
    }

    pub async fn create_session(&self, file_path: String) -> String {

        let token = Uuid::new_v4().to_string();

        // Acquire lock to HashMap
        // if annother tasks holds lock, await (doesnt block thread)
        let mut sessions = self.sessions.lock().await;

        // clone() is used since HashMap::insert takes ownership of the key
        // without it token would move and be unavailable for return 
        sessions.insert(token.clone(), SessionData {
            file_path, 
            used: false,
        });

        token // return ownership of token to caller 
    }

    pub async fn validate_and_mark_used(&self, token: &str) -> Option<String> {

        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.get_mut(token) {
            if !session.used {
                // mark as used FIRST, prevent possible race condition
                session.used = true;

                // Hashmap owns String so clone it to return ownership
                return Some(session.file_path.clone());
            }
        }

        // Token doesnt exists or is already used
        None
    }

    // check if token exists and is not used (read only)
    pub async fn is_valid(&self, token: &str) -> bool {
        let sessions = self.sessions.lock().await;
        sessions.get(token)
            .map(|session| !session.used)
            .unwrap_or(false)
    }
}
