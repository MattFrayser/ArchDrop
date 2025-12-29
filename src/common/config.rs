use serde::{Deserialize, Serialize};

/// Config for chunk transfer
/// Local & Tunnel use different values for optimization
#[derive(Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    pub chunk_size: u64,
    pub concurrency: usize,
}

impl TransferConfig {
    pub fn local() -> Self {
        Self {
            chunk_size: 10 * 1024 * 1024, // 10 MB
            concurrency: 8,
        }
    }

    pub fn tunnel() -> Self {
        Self {
            chunk_size: 1024 * 1024, // 1 MB
            concurrency: 2,
        }
    }
}
