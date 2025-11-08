use tokio::time::{sleep, Duration};
use anyhow::Result;
use tracing::{error, warn};

#[derive(Debug, Clone)]
pub struct RetrySettings {
    pub attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl RetrySettings {
    pub async fn run_with_retry<F, Fut, T>(&self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut delay = self.base_delay_ms;

        for attempt in 1..=self.attempts {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(e) if attempt < self.attempts => {
                    warn!("Attempt {attempt}/{} failed: {e}", self.attempts);
                    sleep(Duration::from_millis(delay)).await;
                    delay = (delay * 2).min(self.max_delay_ms);
                }
                Err(e) => {
                    error!("all {attempt} attempts failed: {e}");
                    return Err(e);
                }
            }
        }
        unreachable!("Retry loop exhausted unexpectedly")
    }
}
