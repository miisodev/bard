//! A single-value cache with a time-to-live.
//!
//! For answers that are expensive to recompute and harmless to serve a moment
//! stale. The licence gate reads Windows Credential Manager several times per
//! answer and the microphone probe enumerates audio devices; both are far more
//! costly than the clock comparison that decides whether the stored answer is
//! still good.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One value, reused until `ttl` has elapsed since it was stored.
///
/// A caller with a mutation of its own must call [`Ttl::invalidate`] rather
/// than wait the TTL out: the window is for changes the cache cannot see, not
/// for ones it made itself.
pub struct Ttl<T> {
    ttl: Duration,
    value: Mutex<Option<(Instant, T)>>,
}

/// Whether a value stored at `at` is still fresh at `now`.
///
/// Split out so the rule is testable without a real clock. A `now` before `at`
/// (a clock that jumped backwards) reads as fresh — `saturating_duration_since`
/// keeps that from panicking, and treating the value as stale would only throw
/// away a good answer.
fn fresh(at: Instant, now: Instant, ttl: Duration) -> bool {
    now.saturating_duration_since(at) < ttl
}

impl<T: Clone> Ttl<T> {
    pub const fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            value: Mutex::new(None),
        }
    }

    /// The stored value while it is fresh, `None` once the TTL has passed.
    ///
    /// A poisoned lock serves the same answer as an empty cache rather than
    /// panicking: a cache must never be the reason a caller cannot proceed.
    pub fn get(&self) -> Option<T> {
        let guard = self
            .value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (at, value) = guard.as_ref()?;
        fresh(*at, Instant::now(), self.ttl).then(|| value.clone())
    }

    /// Store a value as fresh from now, replacing whatever was there.
    pub fn set(&self, value: T) {
        let mut guard = self
            .value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some((Instant::now(), value));
    }

    /// Drop the stored value, so the next `get` recomputes.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn invalidate(&self) {
        let mut guard = self
            .value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_ends_exactly_at_the_ttl() {
        let at = Instant::now();
        let ttl = Duration::from_secs(2);
        assert!(fresh(at, at, ttl));
        assert!(fresh(at, at + ttl - Duration::from_millis(1), ttl));
        assert!(!fresh(at, at + ttl, ttl));
        assert!(!fresh(at, at + ttl + Duration::from_secs(1), ttl));
        // Stored "in the future" — the clock moved backwards — is still fresh.
        assert!(fresh(at + ttl, at, ttl));
    }

    #[test]
    fn a_cache_serves_until_it_is_invalidated() {
        let cache: Ttl<u32> = Ttl::new(Duration::from_secs(60));
        assert_eq!(cache.get(), None);
        cache.set(7);
        assert_eq!(cache.get(), Some(7));
        cache.invalidate();
        assert_eq!(cache.get(), None);
    }
}
