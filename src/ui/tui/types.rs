use crate::common::config::Transport;

/// Status of an individual file transfer
#[derive(Clone, Debug, PartialEq)]
pub enum FileStatus {
    Waiting,
    InProgress(f64), // 0.0 - 100.0
    Complete,
    Failed(String),
}

/// Progress info for a single file
#[derive(Clone, Debug)]
pub struct FileProgress {
    pub filename: String,
    pub status: FileStatus,
}

/// Aggregate transfer progress sent to TUI
#[derive(Clone, Debug, Default)]
pub struct TransferProgress {
    pub files: Vec<FileProgress>,
    pub completed: usize,
    pub total: usize,
}

impl TransferProgress {
    pub fn is_complete(&self) -> bool {
        self.total > 0 && self.completed >= self.total
    }
}

/// Static configuration passed to TUI at startup
#[derive(Clone, Debug)]
pub struct TuiConfig {
    pub is_receiving: bool,
    pub transport: Transport,
    pub url: String,
    pub qr_code: String,
    pub display_name: String,
    pub show_qr: bool,
    pub show_url: bool,
}
