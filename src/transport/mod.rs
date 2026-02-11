pub(crate) mod cloudflare;
pub(crate) mod local;
pub(crate) mod tailscale;
pub(crate) mod tunnel;

use std::future::Future;
use std::time::Duration;

pub(crate) const TRANSPORT_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) async fn with_startup_timeout<F, T>(future: F) -> Result<T, tokio::time::error::Elapsed>
where
    F: Future<Output = T>,
{
    with_timeout(TRANSPORT_STARTUP_TIMEOUT, future).await
}

async fn with_timeout<F, T>(duration: Duration, future: F) -> Result<T, tokio::time::error::Elapsed>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future).await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[tokio::test]
    async fn startup_timeout_wrapper_expires() {
        let result = super::with_timeout(Duration::from_millis(5), async {
            tokio::time::sleep(Duration::from_millis(25)).await;
            42usize
        })
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn startup_timeout_wrapper_allows_fast_future() {
        let result = super::with_startup_timeout(async { 7usize }).await;

        assert!(matches!(result, Ok(7)));
    }
}
