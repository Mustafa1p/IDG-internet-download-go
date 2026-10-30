//! Reduce simultaneous requests after explicit HTTP 429/503 connection refusals.
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct ConnectionGate {
    slots: Arc<Semaphore>,
    limit: AtomicUsize,
}

impl ConnectionGate {
    pub fn new(limit: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(limit.max(1))),
            limit: AtomicUsize::new(limit.max(1)),
        }
    }

    pub async fn acquire(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> anyhow::Result<OwnedSemaphorePermit> {
        loop {
            anyhow::ensure!(
                !cancel.load(Ordering::Relaxed),
                "cancelled waiting for connection slot"
            );
            tokio::select! {
                permit = self.slots.clone().acquire_owned() => return Ok(permit?),
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
    }

    pub fn refused(&self, permit: OwnedSemaphorePermit) {
        // Retire the refused slot. Successful ranges retain their permits;
        // rejected workers queue behind them instead of hammering the host.
        // Keep one slot even if all connections encounter temporary overload.
        let mut limit = self.limit.load(Ordering::Relaxed);
        while limit > 1 {
            match self.limit.compare_exchange_weak(
                limit,
                limit - 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    permit.forget();
                    return;
                }
                Err(current) => limit = current,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn all_refusals_keep_one_slot_and_cancelled_waiters_exit() {
        let gate = ConnectionGate::new(4);
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut permits = Vec::new();
        for _ in 0..4 {
            permits.push(gate.acquire(&cancel).await.unwrap());
        }
        for permit in permits {
            gate.refused(permit);
        }
        let held = gate.acquire(&cancel).await.unwrap();
        cancel.store(true, Ordering::Relaxed);
        assert!(gate.acquire(&cancel).await.is_err());
        drop(held);
    }
}
