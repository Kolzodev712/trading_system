//! Pending immediate reconciliation trigger.
//!
//! `Notify::notify_waiters` is lost when no task is waiting. This type stores a pending
//! request so a busy reconciliation loop runs again as soon as it finishes.

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use tokio::time::Interval;

/// Set-and-notify trigger that retains a request across a busy worker.
#[derive(Debug)]
pub struct ImmediateTrigger {
    pending: AtomicBool,
    notify: Notify,
}

impl ImmediateTrigger {
    pub fn new() -> Self {
        Self {
            pending: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    /// Record a request and wake a waiter if one exists.
    pub fn request(&self) {
        self.pending.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Consume a stored request. Returns true if one was pending.
    pub fn take_pending(&self) -> bool {
        self.pending.swap(false, Ordering::SeqCst)
    }

    /// Return immediately if a request is already pending; otherwise wait for the
    /// interval tick or a new request. `true` means an explicit request caused the wake.
    pub async fn wait_tick_or_request(&self, interval: &mut Interval) -> bool {
        let notified = self.notify.notified();
        tokio::pin!(notified);
        if self.take_pending() {
            return true;
        }
        tokio::select! {
            _ = interval.tick() => false,
            _ = &mut notified => self.take_pending(),
        }
    }
}

impl Default for ImmediateTrigger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn request_while_busy_runs_before_next_interval() {
        let trigger = Arc::new(ImmediateTrigger::new());
        let busy = Arc::new(tokio::sync::Notify::new());
        let finished_busy = Arc::new(tokio::sync::Notify::new());

        let worker = {
            let trigger = Arc::clone(&trigger);
            let busy = Arc::clone(&busy);
            let finished_busy = Arc::clone(&finished_busy);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                let _ = interval.tick().await;
                busy.notify_one();
                tokio::time::sleep(Duration::from_millis(80)).await;
                finished_busy.notify_one();
                let started = Instant::now();
                trigger.wait_tick_or_request(&mut interval).await;
                started.elapsed()
            })
        };

        busy.notified().await;
        trigger.request();
        finished_busy.notified().await;
        let elapsed = tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .expect("worker should finish")
            .expect("join");
        assert!(
            elapsed < Duration::from_secs(2),
            "pending request waited {elapsed:?} instead of running immediately after busy work"
        );
    }

    #[tokio::test]
    async fn take_pending_clears_the_stored_request() {
        let trigger = ImmediateTrigger::new();
        assert!(!trigger.take_pending());
        trigger.request();
        assert!(trigger.take_pending());
        assert!(!trigger.take_pending());
    }
}
