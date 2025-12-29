use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;

/// Tracks transfer progress and automatically updates TUI
pub struct ProgressTracker {
    total_chunks: AtomicU64,
    completed_chunks: AtomicU64,
    progress_sender: watch::Sender<f64>,
}
/// Progress tracking with 99% cap until complete is called.
impl ProgressTracker {
    pub fn new(total_chunks: u64, progress_sender: watch::Sender<f64>) -> Self {
        Self {
            total_chunks: AtomicU64::new(total_chunks),
            completed_chunks: AtomicU64::new(0),
            progress_sender,
        }
    }

    /// Increment completed chunks and automatically update TUI
    /// Returns (completed, total)
    pub fn increment(&self) -> (u64, u64) {
        let completed = self.completed_chunks.fetch_add(1, Ordering::SeqCst) + 1;
        let total = self.total_chunks.load(Ordering::SeqCst);

        // Automatically send progress update to TUI
        self.update_progress(completed, total);

        (completed, total)
    }

    pub fn set_total(&self, total: u64) {
        self.total_chunks.store(total, Ordering::SeqCst);
    }

    pub fn get_progress(&self) -> (u64, u64) {
        let completed = self.completed_chunks.load(Ordering::SeqCst);
        let total = self.total_chunks.load(Ordering::SeqCst);
        (completed, total)
    }

    /// Sets progress to 100%
    pub fn complete(&self) {
        let _ = self.progress_sender.send(100.0);
    }

    fn update_progress(&self, completed: u64, total: u64) {
        let raw_progress = if total > 0 {
            (completed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Cap at 99% until explicit completion
        let _ = self.progress_sender.send(raw_progress.min(99.0));
    }
}

impl Clone for ProgressTracker {
    fn clone(&self) -> Self {
        let (completed, total) = self.get_progress();
        Self {
            total_chunks: AtomicU64::new(total),
            completed_chunks: AtomicU64::new(completed),
            progress_sender: self.progress_sender.clone(),
        }
    }
}
