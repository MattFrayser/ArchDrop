//! Buffer pooling for chunk responses to reduce allocations.

use bytes::Bytes;
use std::sync::{Arc, Mutex};

/// Pool of reusable byte buffers for send-chunk responses.
///
/// Buffers are returned to the pool via `PooledVec::Drop` when Axum finishes
pub struct BufferPool {
    buffers: Mutex<Vec<Vec<u8>>>,
    buffer_capacity: usize,
}

impl BufferPool {
    /// Build a pool with `pool_size` buffers of `buffer_capacity`.
    pub fn new(pool_size: usize, buffer_capacity: usize) -> Arc<Self> {
        let buffers = (0..pool_size)
            .map(|_| Vec::with_capacity(buffer_capacity))
            .collect();
        Arc::new(Self {
            buffers: Mutex::new(buffers),
            buffer_capacity,
        })
    }

    /// Take a reusable buffer, allocating only when pool is empty.
    pub fn take(&self) -> Vec<u8> {
        self.buffers
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(self.buffer_capacity))
    }

    /// Wrap a buffer as `Bytes` that returns it to the pool on drop.
    pub fn wrap(self: &Arc<Self>, buf: Vec<u8>) -> Bytes {
        Bytes::from_owner(PooledVec {
            data: buf,
            pool: Arc::clone(self),
        })
    }

    fn return_buf(&self, mut buf: Vec<u8>) {
        buf.clear();
        // Only reclaim buffers with full capacity (drop undersized last-chunk fallbacks)
        if buf.capacity() >= self.buffer_capacity {
            self.buffers.lock().unwrap().push(buf);
        }
    }
}

/// Owns a buffer and returns it to the pool on drop.
/// Used as the owner type for `Bytes::from_owner`.
struct PooledVec {
    data: Vec<u8>,
    pool: Arc<BufferPool>,
}

impl AsRef<[u8]> for PooledVec {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl Drop for PooledVec {
    fn drop(&mut self) {
        let buf = std::mem::take(&mut self.data);
        self.pool.return_buf(buf);
    }
}

#[cfg(test)]
mod tests {
    use super::BufferPool;

    #[test]
    fn returned_buffer_is_cleared_and_reused() {
        let pool = BufferPool::new(1, 8);

        let mut buf = pool.take();
        buf.extend_from_slice(b"abcd");
        let bytes = pool.wrap(buf);
        assert_eq!(bytes.len(), 4);

        drop(bytes);

        let reused = pool.take();
        assert_eq!(reused.len(), 0, "reused buffer should be cleared");
        assert!(
            reused.capacity() >= 8,
            "reused buffer should preserve pool capacity"
        );
    }

    #[test]
    fn undersized_buffer_is_not_reclaimed() {
        let pool = BufferPool::new(1, 8);

        let baseline = pool.take();
        assert!(baseline.capacity() >= 8);
        drop(pool.wrap(baseline));

        let small = Vec::with_capacity(2);
        drop(pool.wrap(small));

        let first = pool.take();
        let second = pool.take();

        assert!(first.capacity() >= 8);
        assert!(
            second.capacity() >= 8,
            "small returned buffer should not have been reused"
        );
    }
}
