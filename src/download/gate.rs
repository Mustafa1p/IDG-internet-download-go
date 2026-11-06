//! Share origin capacity and queue requests after server connection pressure.
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// An admitted HTTP request retains its slot until its body is consumed/dropped.
#[derive(Debug)]
pub struct LeasedResponse {
    pub response: reqwest::Response,
    pub permit: HostPermit,
}

#[derive(Debug)]
struct HostWaiter {
    ready: tokio::sync::Notify,
    admitted: std::sync::atomic::AtomicBool,
    responding: std::sync::atomic::AtomicBool,
}

#[derive(Debug)]
struct HostState {
    limit: usize,
    active: usize,
    responding: usize,
    waiting: std::collections::VecDeque<Arc<HostWaiter>>,
}

/// Shared, FIFO admission for concurrent downloads from the same HTTP origin.
/// Weak registry entries retain no credentials, paths, query strings or idle budgets.
#[derive(Debug)]
pub struct HostBudget {
    state: std::sync::Mutex<HostState>,
}

#[derive(Debug)]
pub struct HostPermit {
    budget: Arc<HostBudget>,
    waiter: Arc<HostWaiter>,
}

impl HostBudget {
    fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: std::sync::Mutex::new(HostState {
                limit: limit.max(1),
                active: 0,
                responding: 0,
                waiting: std::collections::VecDeque::new(),
            }),
        })
    }

    pub fn for_url(url: &str) -> Arc<Self> {
        type Registry = std::collections::HashMap<String, std::sync::Weak<HostBudget>>;
        static HOSTS: std::sync::OnceLock<std::sync::Mutex<Registry>> = std::sync::OnceLock::new();
        let Ok(url) = reqwest::Url::parse(url) else {
            return Self::new(super::connections::MAX_CONNECTIONS);
        };
        let origin = url.origin().ascii_serialization();
        let mut hosts = HOSTS.get_or_init(Default::default).lock().unwrap();
        if let Some(budget) = hosts.get(&origin).and_then(std::sync::Weak::upgrade) {
            return budget;
        }
        if hosts.len() >= 256 {
            hosts.retain(|_, budget| budget.strong_count() > 0);
        }
        let budget = Self::new(super::connections::MAX_CONNECTIONS);
        hosts.insert(origin, Arc::downgrade(&budget));
        budget
    }

    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize) {
        let state = self.state.lock().unwrap();
        (state.active, state.limit)
    }

    #[cfg(test)]
    pub fn responding_count(&self) -> usize {
        self.state.lock().unwrap().responding
    }

    fn admit(state: &mut HostState) {
        while state.active < state.limit {
            let Some(waiter) = state.waiting.pop_front() else {
                break;
            };
            state.active += 1;
            waiter.admitted.store(true, Ordering::Release);
            waiter.ready.notify_one();
        }
    }

    /// Dropping this future removes even a granted-but-unobserved reservation.
    pub async fn acquire(self: &Arc<Self>) -> HostPermit {
        let permit = HostPermit {
            budget: self.clone(),
            waiter: Arc::new(HostWaiter {
                ready: tokio::sync::Notify::new(),
                admitted: std::sync::atomic::AtomicBool::new(false),
                responding: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        {
            let mut state = self.state.lock().unwrap();
            state.waiting.push_back(permit.waiter.clone());
            Self::admit(&mut state);
        }
        while !permit.waiter.admitted.load(Ordering::Acquire) {
            permit.waiter.ready.notified().await;
        }
        permit
    }
}

impl HostPermit {
    /// Count only requests that have actually received successful headers.
    pub fn response_started(&self) {
        let mut state = self.budget.state.lock().unwrap();
        if !self.waiter.responding.swap(true, Ordering::Relaxed) {
            state.responding += 1;
            // Already-admitted requests can finish their handshakes after a
            // refusal. Their successful headers prove this capacity exists.
            state.limit = state.limit.max(state.responding);
        }
    }

    /// A closed new connection while peers are responding can indicate a TCP
    /// socket cap. Queue later attempts behind those working responses, rather
    /// than exhausting retries while their connections are still occupied.
    pub fn connection_closed(&self) {
        let mut state = self.budget.state.lock().unwrap();
        let peers = state
            .responding
            .saturating_sub(usize::from(self.waiter.responding.load(Ordering::Relaxed)));
        if peers > 0 {
            state.limit = state.limit.min(peers);
        }
    }

    /// Learn from 429/503 only, while preserving all already-admitted bodies.
    /// A redirected response can identify a busy origin learned by another file.
    pub fn refused(&self, final_url: &str) -> Arc<HostBudget> {
        let budget = HostBudget::for_url(final_url);
        {
            let mut state = budget.state.lock().unwrap();
            let other_requests = state
                .active
                .saturating_sub(usize::from(Arc::ptr_eq(&budget, &self.budget)));
            state.limit = state.limit.min(other_requests.max(1));
        }
        budget
    }
}

impl Drop for HostPermit {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap();
        if self.waiter.admitted.load(Ordering::Relaxed) {
            state.active -= 1;
            if self.waiter.responding.load(Ordering::Relaxed) {
                state.responding -= 1;
            }
        } else {
            state
                .waiting
                .retain(|waiter| !Arc::ptr_eq(waiter, &self.waiter));
        }
        HostBudget::admit(&mut state);
    }
}

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
    async fn closed_connections_count_healthy_peers_and_release_their_state() {
        let budget = HostBudget::new(4);
        let first = budget.acquire().await;
        first.connection_closed();
        assert_eq!(
            budget.counts(),
            (1, 4),
            "No healthy peer means no inferred cap"
        );
        first.response_started();
        first.response_started();
        first.connection_closed();
        assert_eq!(
            budget.counts(),
            (1, 4),
            "A request cannot count itself as a peer"
        );
        let pending = budget.acquire().await;
        let failed = budget.acquire().await;
        failed.connection_closed();
        assert_eq!(
            budget.counts(),
            (3, 1),
            "Pending requests are not healthy responses"
        );
        pending.response_started();
        assert_eq!(
            budget.counts(),
            (3, 2),
            "Late successful headers prove capacity"
        );
        drop(failed);
        drop(pending);
        drop(first);
        assert_eq!(budget.counts(), (0, 2));
        assert_eq!(budget.state.lock().unwrap().responding, 0);
        let next = budget.acquire().await;
        drop(next);
        assert_eq!(budget.counts(), (0, 2));
    }

    #[tokio::test]
    async fn host_capacity_reduction_waits_for_bodies_and_grants_fifo() {
        let budget = HostBudget::new(2);
        let first = budget.acquire().await;
        let second = budget.acquire().await;
        let mut third = Box::pin(budget.acquire());
        let mut fourth = Box::pin(budget.acquire());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut third)
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut fourth)
                .await
                .is_err()
        );
        budget.state.lock().unwrap().limit = 1;
        drop(first);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut third)
                .await
                .is_err()
        );
        drop(second);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut fourth)
                .await
                .is_err()
        );
        let third = tokio::time::timeout(Duration::from_millis(50), third)
            .await
            .unwrap();
        drop(third);
        drop(
            tokio::time::timeout(Duration::from_millis(50), fourth)
                .await
                .unwrap(),
        );
        assert_eq!(budget.state.lock().unwrap().active, 0);
    }

    #[tokio::test]
    async fn cancelled_host_waiters_and_unobserved_grants_never_leak_capacity() {
        let budget = HostBudget::new(1);
        let held = budget.acquire().await;
        let mut cancelled = Box::pin(budget.acquire());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut cancelled)
                .await
                .is_err()
        );
        drop(cancelled);
        let mut granted = Box::pin(budget.acquire());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut granted)
                .await
                .is_err()
        );
        drop(held);
        drop(granted);
        drop(
            tokio::time::timeout(Duration::from_millis(50), budget.acquire())
                .await
                .unwrap(),
        );
        assert_eq!(budget.state.lock().unwrap().active, 0);
    }

    #[test]
    fn host_budgets_share_origins_without_sharing_other_origins() {
        let first = HostBudget::for_url("https://one.example.test/a?token=first");
        assert!(Arc::ptr_eq(
            &first,
            &HostBudget::for_url("https://one.example.test:443/b?token=other")
        ));
        assert!(!Arc::ptr_eq(
            &first,
            &HostBudget::for_url("http://one.example.test/b")
        ));
        assert!(!Arc::ptr_eq(
            &first,
            &HostBudget::for_url("https://one.example.test:444/b")
        ));
        assert!(!Arc::ptr_eq(
            &first,
            &HostBudget::for_url("https://two.example.test/b")
        ));
    }
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
