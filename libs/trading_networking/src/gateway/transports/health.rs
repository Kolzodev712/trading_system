use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct TransportMetrics {
    pub queue_capacity: usize,
    pub sent: u64,
    pub dropped: u64,
    pub publish_errors: u64,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct TransportHealth {
    queue_capacity: usize,
    sent: AtomicU64,
    dropped: AtomicU64,
    publish_errors: AtomicU64,
    last_error: RwLock<Option<String>>,
}

impl TransportHealth {
    pub fn new(queue_capacity: usize) -> Self {
        Self {
            queue_capacity,
            sent: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            publish_errors: AtomicU64::new(0),
            last_error: RwLock::new(None),
        }
    }

    pub fn inc_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_publish_error(&self, err: impl Into<String>) {
        self.publish_errors.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = self.last_error.write() {
            *guard = Some(err.into());
        }
    }

    pub fn snapshot(&self) -> TransportMetrics {
        let last_error = self
            .last_error
            .read()
            .ok()
            .and_then(|g| g.as_ref().cloned());

        TransportMetrics {
            queue_capacity: self.queue_capacity,
            sent: self.sent.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            publish_errors: self.publish_errors.load(Ordering::Relaxed),
            last_error,
        }
    }
}

pub type SharedTransportHealth = Arc<TransportHealth>;
