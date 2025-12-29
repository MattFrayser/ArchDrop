use crate::common::{
    manifest::{FileEntry, Manifest},
    session_core::SessionImpl,
    session_trait::Session,
};
use crate::crypto::types::EncryptionKey;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Send-specific session
/// Composes SessionImpl (auth + crypto) + send-specific state (manifest, deduplication)
pub struct SendSession {
    core: SessionImpl,
    manifest: Manifest,
    total_chunks: AtomicU64,
    chunks_sent: Arc<AtomicU64>,
    sent_chunks: Arc<DashMap<(usize, usize), ()>>, // Deduplication tracking
}

impl SendSession {
    pub fn new(manifest: Manifest, session_key: EncryptionKey, total_chunks: u64) -> Self {
        Self {
            core: SessionImpl::new(session_key),
            manifest,
            total_chunks: AtomicU64::new(total_chunks),
            chunks_sent: Arc::new(AtomicU64::new(0)),
            sent_chunks: Arc::new(DashMap::new()),
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn get_file(&self, index: usize) -> Option<&FileEntry> {
        self.manifest.files.get(index)
    }

    // Send-specific progress tracking
    pub fn increment_sent_chunk(&self) -> (u64, u64) {
        let new_count = self.chunks_sent.fetch_add(1, Ordering::SeqCst) + 1;
        let total = self.total_chunks.load(Ordering::SeqCst);
        (new_count, total)
    }

    // Deduplication methods (send-specific)
    pub fn has_chunk_been_sent(&self, file_index: usize, chunk_index: usize) -> bool {
        self.sent_chunks.contains_key(&(file_index, chunk_index))
    }

    pub fn mark_chunk_sent(&self, file_index: usize, chunk_index: usize) -> bool {
        self.sent_chunks
            .insert((file_index, chunk_index), ())
            .is_none()
    }

    pub fn unique_chunks_sent(&self) -> usize {
        self.sent_chunks.len()
    }

    pub fn get_chunks_sent(&self) -> u64 {
        self.chunks_sent.load(Ordering::SeqCst)
    }

    pub fn get_total_chunks(&self) -> u64 {
        self.total_chunks.load(Ordering::SeqCst)
    }
}

// Implement Session trait via delegation to core
impl Session for SendSession {
    fn token(&self) -> &str {
        self.core.token()
    }

    fn session_key(&self) -> &crate::crypto::types::EncryptionKey {
        self.core.session_key()
    }

    fn cipher(&self) -> &Arc<aes_gcm::Aes256Gcm> {
        self.core.cipher()
    }

    fn session_key_b64(&self) -> String {
        self.core.session_key_b64()
    }

    fn claim(&self, token: &str, client_id: &str) -> bool {
        self.core.claim(token, client_id)
    }

    fn is_active(&self, token: &str, client_id: &str) -> bool {
        self.core.is_active(token, client_id)
    }

    fn complete(&self, token: &str, client_id: &str) -> bool {
        self.core.complete(token, client_id)
    }
}

impl Clone for SendSession {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            manifest: self.manifest.clone(),
            total_chunks: AtomicU64::new(self.total_chunks.load(Ordering::SeqCst)),
            chunks_sent: self.chunks_sent.clone(),
            sent_chunks: self.sent_chunks.clone(),
        }
    }
}
