//! Shared send-session state and transfer-state implementation.

use crate::common::config::TransferSettings;
use crate::common::{manifest::FileEntry, Manifest, Session, TransferState};
use crate::crypto::types::EncryptionKey;
use crate::send::buffer_pool::BufferPool;
use crate::send::file_handle::SendFileHandle;
use crate::server::progress::ProgressTracker;
use dashmap::DashMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Cheaply cloned handle to send state stored behind `Arc`.
#[derive(Clone)]
pub struct SendAppState {
    inner: Arc<SendAppStateInner>,
}

/// Send-specific application state for handlers and progress tracking
pub struct SendAppStateInner {
    pub session: Session,
    pub manifest: Manifest,
    pub progress: Arc<ProgressTracker>,
    pub file_handles: Arc<DashMap<usize, Arc<SendFileHandle>>>,
    pub buffer_pool: Arc<BufferPool>,
    pub config: TransferSettings,
    chunks_sent: Arc<AtomicU64>,
    sent_chunks: Arc<DashMap<(usize, usize), ()>>,
    total_chunks: Arc<AtomicU64>,
}

impl Deref for SendAppState {
    type Target = SendAppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl SendAppState {
    /// Build send state from session data, manifest, and transfer settings.
    pub fn new(
        session_key: EncryptionKey,
        manifest: Manifest,
        total_chunks: u64,
        progress: Arc<ProgressTracker>,
        config: TransferSettings,
    ) -> Self {
        // +16 bytes for AES-GCM tag appended during encrypt_in_place
        let buf_capacity = config.chunk_size as usize + 16;
        let pool_size = config.concurrency;

        Self {
            inner: Arc::new(SendAppStateInner {
                session: Session::new(session_key),
                manifest,
                progress,
                file_handles: Arc::new(DashMap::new()),
                buffer_pool: BufferPool::new(pool_size, buf_capacity),
                config,
                chunks_sent: Arc::new(AtomicU64::new(0)),
                sent_chunks: Arc::new(DashMap::new()),
                total_chunks: Arc::new(AtomicU64::new(total_chunks)),
            }),
        }
    }

    /// Return the transfer manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Return a file entry by manifest index.
    pub fn get_file(&self, index: usize) -> Option<&FileEntry> {
        self.manifest.files.get(index)
    }

    /// Increment sent-chunk counter and return `(sent, total)`.
    pub fn increment_sent_chunk(&self) -> (u64, u64) {
        let new_count = self.chunks_sent.fetch_add(1, Ordering::SeqCst) + 1;
        let total = self.total_chunks.load(Ordering::SeqCst);
        (new_count, total)
    }

    /// Return whether this file/chunk pair was already sent.
    pub fn has_chunk_been_sent(&self, file_index: usize, chunk_index: usize) -> bool {
        self.sent_chunks.contains_key(&(file_index, chunk_index))
    }

    /// Mark a file/chunk pair as sent; true if newly inserted.
    pub fn mark_chunk_sent(&self, file_index: usize, chunk_index: usize) -> bool {
        self.sent_chunks
            .insert((file_index, chunk_index), ())
            .is_none()
    }

    /// Return count of unique file/chunk pairs sent.
    pub fn unique_chunks_sent(&self) -> usize {
        self.sent_chunks.len()
    }

    /// Return total chunk responses served.
    pub fn get_chunks_sent(&self) -> u64 {
        self.chunks_sent.load(Ordering::SeqCst)
    }

    /// Return expected total chunk count for this transfer.
    pub fn get_total_chunks(&self) -> u64 {
        self.total_chunks.load(Ordering::SeqCst)
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

    fn session(&self) -> &Session {
        &self.session
    }

    fn service_path(&self) -> &'static str {
        "send"
    }

    fn is_receiving(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::TransferSettings;

    #[test]
    fn clone_shares_total_chunks_atomic() {
        let state = SendAppState::new(
            EncryptionKey::new(),
            Manifest {
                files: Vec::new(),
                config: TransferSettings {
                    chunk_size: 1024,
                    concurrency: 1,
                },
            },
            3,
            Arc::new(ProgressTracker::new()),
            TransferSettings {
                chunk_size: 1024,
                concurrency: 1,
            },
        );

        let cloned = state.clone();

        state.total_chunks.store(9, Ordering::SeqCst);

        assert_eq!(cloned.get_total_chunks(), 9);
    }
}
