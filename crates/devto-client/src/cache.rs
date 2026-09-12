//! A small read-through cache.
//!
//! Not an optimization. At 30 reads a minute, a model that fetches the tag list twice in
//! one turn has spent 7% of the budget on the same bytes. Entries are kept by request key
//! with a per-endpoint TTL, and the clock is injected so expiry is testable.

use std::collections::HashMap;

/// How long a response stays usable. Chosen per endpoint by how fast the data actually
/// moves, not by a single global number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ttl(pub u64);

impl Ttl {
    /// The authenticated identity cannot change during a session.
    pub const SESSION: Ttl = Ttl(u64::MAX);
    /// Taxonomy and instance shape move slowly.
    pub const DAY: Ttl = Ttl(24 * 60 * 60 * 1_000);
    /// Long enough to absorb a model re-reading what it just fetched.
    pub const MINUTE: Ttl = Ttl(60 * 1_000);
    /// Analytics are served `Cache-Control: no-store` upstream; honour that.
    pub const NONE: Ttl = Ttl(0);
}

#[derive(Debug, Clone)]
struct Entry {
    body: String,
    stored_at: u64,
    ttl: u64,
}

#[derive(Debug, Default)]
pub struct ResponseCache {
    entries: HashMap<String, Entry>,
    pub hits: u64,
    pub misses: u64,
}

impl ResponseCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&mut self, key: &str, now: u64) -> Option<String> {
        let fresh = match self.entries.get(key) {
            Some(entry) => is_fresh(entry, now),
            None => false,
        };
        if fresh {
            self.hits += 1;
            return self.entries.get(key).map(|e| e.body.clone());
        }
        // A stale entry is dead weight; drop it rather than let the map grow.
        self.entries.remove(key);
        self.misses += 1;
        None
    }

    pub fn put(&mut self, key: &str, body: &str, ttl: Ttl, now: u64) {
        if ttl.0 == 0 {
            return;
        }
        self.entries.insert(
            key.to_string(),
            Entry {
                body: body.to_string(),
                stored_at: now,
                ttl: ttl.0,
            },
        );
    }

    /// Drop everything. A write invalidates whatever the caller thought it knew.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Fresh means `now` lies in `[stored_at, stored_at + ttl)`.
///
/// A `now` before `stored_at` means the clock went backwards — `SystemTime` can, unlike a
/// monotonic one — and is treated as stale. Erring toward a wasted request is safer than
/// erring toward serving something whose age we cannot compute.
fn is_fresh(entry: &Entry, now: u64) -> bool {
    if entry.ttl == u64::MAX {
        return true;
    }
    now >= entry.stored_at && now - entry.stored_at < entry.ttl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_entry_comes_back_until_its_ttl_expires() {
        let mut cache = ResponseCache::new();
        cache.put("k", "value", Ttl(1_000), 0);

        assert_eq!(cache.get("k", 0).as_deref(), Some("value"));
        assert_eq!(cache.get("k", 999).as_deref(), Some("value"));
        assert_eq!(cache.get("k", 1_000), None, "the TTL is exclusive");
        assert_eq!(cache.hits, 2);
        assert_eq!(cache.misses, 1);
    }

    #[test]
    fn an_expired_entry_is_dropped_rather_than_kept() {
        let mut cache = ResponseCache::new();
        cache.put("k", "value", Ttl(10), 0);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("k", 100), None);
        assert!(cache.is_empty(), "a stale entry must not linger in the map");
    }

    #[test]
    fn a_session_entry_never_expires() {
        let mut cache = ResponseCache::new();
        cache.put("me", "identity", Ttl::SESSION, 0);
        assert_eq!(cache.get("me", u64::MAX - 1).as_deref(), Some("identity"));
    }

    /// Analytics carry `Cache-Control: no-store`. Storing them would report stale numbers
    /// as current, which is worse than spending the request.
    #[test]
    fn a_zero_ttl_is_never_stored() {
        let mut cache = ResponseCache::new();
        cache.put("analytics", "numbers", Ttl::NONE, 0);
        assert!(cache.is_empty());
        assert_eq!(cache.get("analytics", 0), None);
    }

    #[test]
    fn a_missing_key_is_a_miss() {
        let mut cache = ResponseCache::new();
        assert_eq!(cache.get("nothing", 0), None);
        assert_eq!(cache.misses, 1);
        assert_eq!(cache.hits, 0);
    }

    #[test]
    fn clearing_drops_everything() {
        let mut cache = ResponseCache::new();
        cache.put("a", "1", Ttl::DAY, 0);
        cache.put("b", "2", Ttl::DAY, 0);
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn a_later_write_replaces_an_earlier_one() {
        let mut cache = ResponseCache::new();
        cache.put("k", "old", Ttl(1_000), 0);
        cache.put("k", "new", Ttl(1_000), 500);
        assert_eq!(cache.get("k", 1_400).as_deref(), Some("new"));
    }

    #[test]
    fn the_ttl_constants_are_the_durations_they_claim() {
        assert_eq!(Ttl::MINUTE.0, 60_000);
        assert_eq!(Ttl::DAY.0, 86_400_000);
        assert_eq!(Ttl::NONE.0, 0);
        assert_eq!(Ttl::SESSION.0, u64::MAX);
    }

    #[test]
    fn a_populated_cache_is_not_empty() {
        let mut cache = ResponseCache::new();
        assert!(cache.is_empty());
        cache.put("k", "v", Ttl::DAY, 0);
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
    }

    /// A clock that runs backwards cannot be used to compute an age, so the entry is
    /// treated as stale rather than served with an age we cannot justify.
    #[test]
    fn a_backwards_clock_invalidates_rather_than_extends() {
        let mut cache = ResponseCache::new();
        cache.put("k", "v", Ttl(1_000), 5_000);
        assert_eq!(cache.get("k", 4_999), None);
    }

    #[hegel::test]
    fn a_stored_value_is_returned_exactly_within_its_window(tc: hegel::TestCase) {
        let ttl = tc.draw(
            hegel::generators::integers::<u64>()
                .min_value(1)
                .max_value(100_000),
        );
        let stored_at = tc.draw(
            hegel::generators::integers::<u64>()
                .min_value(0)
                .max_value(1_000_000),
        );
        let read_at = tc.draw(
            hegel::generators::integers::<u64>()
                .min_value(0)
                .max_value(2_000_000),
        );

        let mut cache = ResponseCache::new();
        cache.put("k", "value", Ttl(ttl), stored_at);

        let expected_hit = read_at >= stored_at && read_at - stored_at < ttl;
        assert_eq!(
            cache.get("k", read_at).is_some(),
            expected_hit,
            "ttl={ttl} stored_at={stored_at} read_at={read_at}"
        );
    }
}
