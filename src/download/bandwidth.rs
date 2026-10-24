//! One pacing schedule shared by every segment of a single download.
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

pub struct BandwidthLimiter {
    rate: AtomicU64,
    next: Mutex<Instant>,
}
impl Default for BandwidthLimiter {
    fn default() -> Self {
        Self::new(0)
    }
}
impl BandwidthLimiter {
    pub fn new(bytes_per_second: u64) -> Self {
        Self {
            rate: AtomicU64::new(bytes_per_second),
            next: Mutex::new(Instant::now()),
        }
    }
    pub fn rate(&self) -> u64 {
        self.rate.load(Ordering::Relaxed)
    }
    pub async fn acquire(&self, bytes: usize) {
        let rate = self.rate();
        if rate == 0 || bytes == 0 {
            return;
        }
        let deadline = {
            let mut next = self.next.lock().await;
            *next =
                (*next).max(Instant::now()) + Duration::from_secs_f64(bytes as f64 / rate as f64);
            *next
        };
        tokio::time::sleep_until(deadline).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn segments_share_one_limit_and_unlimited_has_no_delay() {
        let limiter = BandwidthLimiter::new(200_000);
        let started = Instant::now();
        tokio::join!(limiter.acquire(20_000), limiter.acquire(20_000));
        assert!(started.elapsed() >= Duration::from_millis(190));
        let unlimited = BandwidthLimiter::default();
        assert_eq!(unlimited.rate(), 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), unlimited.acquire(usize::MAX))
                .await
                .is_ok()
        );
    }
}
