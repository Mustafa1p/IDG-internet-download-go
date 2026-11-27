//! Diagnostic only: try optional capacity when completion is still distant.
use std::time::{Duration, Instant};

pub struct Adaptive {
    maximum: usize,
    target: usize,
    accepted: usize,
    baseline: f64,
    previous_bytes: u64,
    sampled: Instant,
    window: Duration,
    trial: bool,
    settled: bool,
    pub decisions: Vec<serde_json::Value>,
}

impl Adaptive {
    pub fn new(maximum: usize, headers_ms: u128) -> Self {
        Self {
            maximum,
            target: 0,
            accepted: 0,
            baseline: 0.0,
            previous_bytes: 0,
            sampled: Instant::now(),
            window: Duration::from_millis(80 + headers_ms.min(200) as u64),
            trial: false,
            settled: maximum == 0,
            decisions: Vec::new(),
        }
    }

    pub fn deadline(&self) -> tokio::time::Instant {
        tokio::time::Instant::from_std(self.sampled + self.window)
    }

    pub fn observe(&mut self, bytes: u64, total: u64, optional_failed: bool) -> Option<usize> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.sampled).as_secs_f64();
        let rate = bytes.saturating_sub(self.previous_bytes) as f64 / elapsed;
        self.sampled = now;
        self.previous_bytes = bytes;
        if self.settled || bytes >= total {
            return None;
        }
        if optional_failed {
            // A coincidental improvement in the original stream does not
            // authorize more requests after a terminal helper refusal.
            self.target = self.accepted;
            self.settled = true;
            self.decisions.push(serde_json::json!({"action":"rollback","reason":"optional_failure","helpers":self.target,"bytes":bytes,"bytes_per_second":rate}));
            return Some(self.target);
        }
        if self.trial {
            if rate <= self.baseline * 1.10 {
                self.target = self.accepted;
                self.settled = true;
                self.decisions.push(serde_json::json!({"action":"rollback","helpers":self.target,"bytes":bytes,"bytes_per_second":rate,"baseline":self.baseline}));
                return Some(self.target);
            }
            self.accepted = self.target;
        }
        // Avoid extra requests for a stream likely to finish within 600 ms.
        // A zero-rate stream can still try optional capacity after this window.
        if self.target == self.maximum || rate > 0.0 && (total - bytes) as f64 / rate < 0.6 {
            self.trial = false;
            self.baseline = rate;
            return None;
        }
        self.baseline = rate;
        self.target = if self.target == 0 { 3 } else { self.target * 2 }.min(self.maximum);
        self.trial = true;
        self.decisions.push(serde_json::json!({"action":"trial","helpers":self.target,"bytes":bytes,"bytes_per_second":rate}));
        Some(self.target)
    }
}
