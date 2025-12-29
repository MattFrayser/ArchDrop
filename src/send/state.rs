use crate::common::config::TransferConfig;
use crate::common::{Session, TransferState};
use crate::send::file_handle::SendFileHandle;
use crate::send::session::SendSession;
use crate::server::progress::ProgressTracker;
use dashmap::DashMap;
use std::sync::Arc;

/// Send-specific application state
/// Passed to all send handlers via Axum State extractor
#[derive(Clone)]
pub struct SendAppState {
    pub session: SendSession,
    pub progress: ProgressTracker,
    pub file_handles: Arc<DashMap<usize, Arc<SendFileHandle>>>,
    pub config: TransferConfig,
}

impl SendAppState {
    pub fn new(session: SendSession, progress: ProgressTracker, config: TransferConfig) -> Self {
        Self {
            session,
            progress,
            file_handles: Arc::new(DashMap::new()),
            config,
        }
    }
}

#[async_trait::async_trait]
impl TransferState for SendAppState {
    fn transfer_count(&self) -> usize {
        self.file_handles.len()
    }

    async fn cleanup(&self) {
        let count = self.file_handles.len();
        if count > 0 {
            tracing::debug!("Cleaning up {} send session(s)", count);
        }
        self.file_handles.clear();
    }

    fn session(&self) -> &dyn Session {
        &self.session
    }

    fn service_path(&self) -> &'static str {
        "send"
    }

    fn is_receiving(&self) -> bool {
        false
    }
}
