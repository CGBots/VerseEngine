//! Limitation de débit en mémoire (seau à jetons par clé).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_KEYS: usize = 100_000;

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

#[derive(Debug)]
pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    /// `capacity` = rafale max, `refill_per_sec` = débit soutenu.
    pub fn new(capacity: u32, refill_per_sec: f64) -> Self {
        Self { capacity: capacity as f64, refill_per_sec, buckets: Mutex::new(HashMap::new()) }
    }

    pub fn check(&self, key: &str) -> Result<(), Duration> { self.check_at(key, Instant::now()) }

    /// Consomme un jeton ; en cas de refus, renvoie le délai avant le prochain jeton.
    pub fn check_at(&self, key: &str, now: Instant) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().unwrap_or_else(|p| p.into_inner());
        if buckets.len() >= MAX_KEYS && !buckets.contains_key(key) {
            // Purge des seaux pleins (inactifs) pour borner la mémoire.
            let (cap, rate) = (self.capacity, self.refill_per_sec);
            buckets.retain(|_, b| b.tokens + now.saturating_duration_since(b.last).as_secs_f64() * rate < cap);
            if buckets.len() >= MAX_KEYS {
                return Err(Duration::from_secs(1));
            }
        }
        let b = buckets.entry(key.to_owned()).or_insert(Bucket { tokens: self.capacity, last: now });
        let elapsed = now.saturating_duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            Ok(())
        } else {
            let missing = 1.0 - b.tokens;
            Err(Duration::from_secs_f64((missing / self.refill_per_sec).max(0.001)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_then_refuse_then_refill() {
        let rl = RateLimiter::new(3, 1.0);
        let t0 = Instant::now();
        for _ in 0..3 { assert!(rl.check_at("a", t0).is_ok()); }
        let retry = rl.check_at("a", t0).unwrap_err();
        assert!(retry <= Duration::from_secs(1) && retry > Duration::ZERO);
        assert!(rl.check_at("a", t0 + Duration::from_millis(1100)).is_ok());
        assert!(rl.check_at("a", t0 + Duration::from_millis(1100)).is_err());
    }

    #[test]
    fn keys_are_isolated() {
        let rl = RateLimiter::new(1, 0.1);
        let t0 = Instant::now();
        assert!(rl.check_at("a", t0).is_ok());
        assert!(rl.check_at("a", t0).is_err());
        assert!(rl.check_at("b", t0).is_ok());
    }

    #[test]
    fn never_exceeds_capacity_after_idle() {
        let rl = RateLimiter::new(2, 10.0);
        let t0 = Instant::now();
        let later = t0 + Duration::from_secs(3600);
        assert!(rl.check_at("a", later).is_ok());
        assert!(rl.check_at("a", later).is_ok());
        assert!(rl.check_at("a", later).is_err());
    }
}
