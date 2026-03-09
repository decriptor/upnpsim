use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A virtual clock that can be shifted forward in time.
/// Uses an atomic offset so reads are lock-free from all tasks.
#[derive(Clone)]
pub struct VirtualClock {
    offset_ms: Arc<AtomicI64>,
    start_real_ms: i64,
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualClock {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        Self {
            offset_ms: Arc::new(AtomicI64::new(0)),
            start_real_ms: now,
        }
    }

    /// Current virtual time as epoch milliseconds.
    pub fn now_ms(&self) -> i64 {
        let real = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        real + self.offset_ms.load(Ordering::Relaxed)
    }

    /// Current virtual time as epoch seconds.
    pub fn now_secs(&self) -> i64 {
        self.now_ms() / 1000
    }

    /// Virtual uptime in seconds since this clock was created.
    pub fn uptime_secs(&self) -> i64 {
        let real = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let virtual_now = real + self.offset_ms.load(Ordering::Relaxed);
        (virtual_now - self.start_real_ms) / 1000
    }

    /// Shift the virtual clock forward by the given duration.
    pub fn shift_forward(&self, duration: Duration) {
        let ms = duration.as_millis() as i64;
        self.offset_ms.fetch_add(ms, Ordering::Relaxed);
    }

    /// Current offset in milliseconds.
    pub fn offset_ms(&self) -> i64 {
        self.offset_ms.load(Ordering::Relaxed)
    }
}
