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
    trial: std::sync::atomic::AtomicBool,
    allow_probe: bool,
}

#[derive(Debug)]
struct HostState {
    limit: usize,
    active: usize,
    responding: usize,
    closed_failures: u8,
    probing: bool,
    next_probe: Option<tokio::time::Instant>,
    probe_backoff: Duration,
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
                closed_failures: 0,
                probing: false,
                next_probe: None,
                probe_backoff: Duration::from_millis(50),
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
        if !state.probing
            && state.active == state.limit
            && state.limit < super::connections::MAX_CONNECTIONS
            && state.responding > 0
            && state
                .next_probe
                .is_some_and(|at| at <= tokio::time::Instant::now())
            && let Some(index) = state.waiting.iter().position(|waiter| waiter.allow_probe)
        {
            // Only an idempotent request may test one extra connection. Forms
            // retain ordinary FIFO admission until a real slot becomes free.
            let waiter = state.waiting.remove(index).unwrap();
            state.active += 1;
            state.probing = true;
            state.next_probe = Some(tokio::time::Instant::now() + state.probe_backoff);
            waiter.trial.store(true, Ordering::Relaxed);
            waiter.admitted.store(true, Ordering::Release);
            waiter.ready.notify_one();
        }
    }

    fn wake_waiters(state: &HostState) {
        if let Some(waiter) = state.waiting.front() {
            waiter.ready.notify_one();
        }
        // A form at the FIFO head cannot arm a speculative probe timer. Wake
        // the oldest safe request too, so failed/cancelled trials can recover
        // capacity without sending that form or leaving its followers asleep.
        if let Some(waiter) = state.waiting.iter().find(|waiter| waiter.allow_probe) {
            waiter.ready.notify_one();
        }
    }

    #[cfg(test)]
    pub async fn acquire(self: &Arc<Self>) -> HostPermit {
        self.acquire_request(false).await
    }

    /// Dropping this future removes even a granted-but-unobserved reservation.
    pub async fn acquire_request(self: &Arc<Self>, allow_probe: bool) -> HostPermit {
        let permit = HostPermit {
            budget: self.clone(),
            waiter: Arc::new(HostWaiter {
                ready: tokio::sync::Notify::new(),
                admitted: std::sync::atomic::AtomicBool::new(false),
                responding: std::sync::atomic::AtomicBool::new(false),
                trial: std::sync::atomic::AtomicBool::new(false),
                allow_probe,
            }),
        };
        {
            let mut state = self.state.lock().unwrap();
            state.waiting.push_back(permit.waiter.clone());
            Self::admit(&mut state);
        }
        while !permit.waiter.admitted.load(Ordering::Acquire) {
            let next_probe = {
                let mut state = self.state.lock().unwrap();
                Self::admit(&mut state);
                if allow_probe
                    && !state.probing
                    && state.responding > 0
                    && state.active == state.limit
                    && state.limit < super::connections::MAX_CONNECTIONS
                {
                    state.next_probe
                } else {
                    None
                }
            };
            if permit.waiter.admitted.load(Ordering::Acquire) {
                break;
            }
            match next_probe {
                Some(at) => tokio::select! {
                    _ = permit.waiter.ready.notified() => {},
                    _ = tokio::time::sleep_until(at) => {},
                },
                None => permit.waiter.ready.notified().await,
            }
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
            state.closed_failures = 0;
            // Already-admitted requests can finish their handshakes after a
            // refusal. Their successful headers prove this capacity exists.
            state.limit = state.limit.max(state.responding);
            if self.waiter.trial.swap(false, Ordering::Relaxed) {
                state.probing = false;
                state.probe_backoff = Duration::from_millis(50);
                state.next_probe = Some(tokio::time::Instant::now() + state.probe_backoff);
            }
            HostBudget::admit(&mut state);
            HostBudget::wake_waiters(&state);
        }
    }

    /// A closed new connection while peers are responding can indicate a TCP
    /// socket cap. Queue later attempts behind those working responses, rather
    /// than exhausting retries while their connections are still occupied.
    pub fn connection_closed(&self) -> bool {
        let mut state = self.budget.state.lock().unwrap();
        let peers = state
            .responding
            .saturating_sub(usize::from(self.waiter.responding.load(Ordering::Relaxed)));
        if peers == 0 {
            state.closed_failures = 0;
            return false;
        }
        state.closed_failures = state.closed_failures.saturating_add(1);
        if state.next_probe.is_none() && state.closed_failures < 2 {
            return false;
        }
        state.limit = state.limit.min(peers);
        if self.waiter.trial.load(Ordering::Relaxed) {
            state.probe_backoff = state
                .probe_backoff
                .saturating_mul(2)
                .min(Duration::from_secs(1));
        }
        state.next_probe = Some(tokio::time::Instant::now() + state.probe_backoff);
        HostBudget::wake_waiters(&state);
        true
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
            // Explicit HTTP pressure does not authorize speculative capacity
            // probes. Its existing retry/admission policy remains in force.
            state.next_probe = None;
            state.closed_failures = 0;
        }
        budget
    }
}

impl Drop for HostPermit {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap();
        if self.waiter.admitted.load(Ordering::Relaxed) {
            state.active -= 1;
            if self.waiter.trial.load(Ordering::Relaxed) {
                state.probing = false;
            }
            if self.waiter.responding.load(Ordering::Relaxed) {
                state.responding -= 1;
            }
        } else {
            state
                .waiting
                .retain(|waiter| !Arc::ptr_eq(waiter, &self.waiter));
        }
        HostBudget::admit(&mut state);
        HostBudget::wake_waiters(&state);
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
        assert!(
            !failed.connection_closed(),
            "One isolated close is not capacity evidence"
        );
        assert_eq!(budget.counts(), (3, 4));
        let repeated = budget.acquire().await;
        assert!(repeated.connection_closed());
        assert_eq!(
            budget.counts(),
            (4, 1),
            "Pending requests are not healthy responses"
        );
        pending.response_started();
        assert_eq!(
            budget.counts(),
            (4, 2),
            "Late successful headers prove capacity"
        );
        drop(repeated);
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
    async fn capacity_trials_are_single_idempotent_requests_and_do_not_send_waiting_forms() {
        let budget = HostBudget::new(4);
        let first = budget.acquire().await;
        let second = budget.acquire().await;
        first.response_started();
        second.response_started();
        for expected in [false, true] {
            let failed = budget.acquire().await;
            assert_eq!(failed.connection_closed(), expected);
            drop(failed);
        }
        assert_eq!(budget.counts(), (2, 2));
        let mut form = Box::pin(budget.acquire_request(false));
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut form)
                .await
                .is_err()
        );
        let trial = tokio::time::timeout(Duration::from_secs(1), budget.acquire_request(true))
            .await
            .unwrap();
        assert!(trial.waiter.trial.load(Ordering::Relaxed));
        assert_eq!(budget.counts(), (3, 2));
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut form)
                .await
                .is_err()
        );
        let mut another_trial = Box::pin(budget.acquire_request(true));
        assert!(
            tokio::time::timeout(Duration::from_millis(60), &mut another_trial)
                .await
                .is_err(),
            "Only one speculative connection may be in flight"
        );
        assert!(trial.connection_closed());
        drop(trial);
        let recovered = tokio::time::timeout(Duration::from_secs(1), another_trial)
            .await
            .expect("A waiting form must not hide the next safe capacity probe");
        recovered.response_started();
        assert_eq!(budget.counts(), (3, 3));
        drop(recovered);
        let form = tokio::time::timeout(Duration::from_secs(1), form)
            .await
            .unwrap();
        assert!(!form.waiter.trial.load(Ordering::Relaxed));
        drop(form);
        drop(first);
        drop(second);
        assert_eq!(budget.counts().0, 0);
        assert!(!budget.state.lock().unwrap().probing);
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
