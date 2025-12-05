use anyhow::{Context, Result};
use std::fs::File;
use std::sync::Arc;

// Implement required traits based on OS
#[cfg(unix)]
use std::os::unix::fs::FileExt;

pub fn read_chunk_at_position(file_handle: &Arc<File>, start: u64, len: usize) -> Result<Vec<u8>> {
    let mut buffer = vec![0u8; len];

    #[cfg(unix)]
    file_handle
        .read_exact_at(&mut buffer, start)
        .context(format!("Failed to read chunk (unix) at offset {}", start))?;

    Ok(buffer)
}
