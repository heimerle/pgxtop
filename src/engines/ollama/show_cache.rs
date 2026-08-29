//! Cache for `/api/show`, which is fetched lazily (only when the user opens
//! the detail overlay) and never on the poll tick.
//!
//! Keys are content-addressed where possible: a sha256 digest uniquely
//! identifies the weights, so a digest-keyed hit can never go stale and is
//! kept for the process lifetime. Name-keyed entries (older Ollama, or a
//! response without a digest) expire.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use indexmap::IndexMap;

use crate::models::inference::ModelDetail;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowKey(String);

impl ShowKey {
    pub fn new(name: &str, digest: Option<&str>) -> Self {
        match digest {
            Some(d) if !d.is_empty() => ShowKey(format!("d:{d}")),
            _ => ShowKey(format!("n:{name}")),
        }
    }

    pub fn is_digest_keyed(&self) -> bool {
        self.0.starts_with("d:")
    }
}

#[derive(Debug, Clone)]
enum Entry {
    Hit {
        detail: Arc<ModelDetail>,
        fetched_at: Instant,
    },
    /// Negative cache with exponential backoff. Never permanent: an Ollama
    /// restart can make a previously unshowable model showable again.
    Miss {
        failures: u32,
        retry_after: Instant,
    },
    /// A request is already running — do not start a second one.
    InFlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Cached detail is available.
    Hit,
    /// A fetch is running; the UI should render "loading…".
    Loading,
    /// Nothing cached and no fetch running; the caller should start one.
    Missing,
    /// Recently failed and still inside the backoff window.
    Backoff,
}

pub struct ShowCache {
    entries: IndexMap<ShowKey, Entry>,
    capacity: usize,
}

impl Default for ShowCache {
    fn default() -> Self {
        Self::new(128)
    }
}

impl ShowCache {
    const NAME_TTL: Duration = Duration::from_secs(600);
    const BACKOFF_BASE: Duration = Duration::from_secs(30);
    const BACKOFF_MAX: Duration = Duration::from_secs(600);

    pub fn new(capacity: usize) -> Self {
        Self {
            entries: IndexMap::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn get(&self, k: &ShowKey, now: Instant) -> Option<Arc<ModelDetail>> {
        match self.entries.get(k) {
            Some(Entry::Hit { detail, fetched_at }) => {
                if k.is_digest_keyed() || now.duration_since(*fetched_at) < Self::NAME_TTL {
                    Some(detail.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn lookup(&self, k: &ShowKey, now: Instant) -> Lookup {
        match self.entries.get(k) {
            Some(Entry::Hit { fetched_at, .. }) => {
                if k.is_digest_keyed() || now.duration_since(*fetched_at) < Self::NAME_TTL {
                    Lookup::Hit
                } else {
                    Lookup::Missing
                }
            }
            Some(Entry::InFlight) => Lookup::Loading,
            Some(Entry::Miss { retry_after, .. }) => {
                if now >= *retry_after {
                    Lookup::Missing
                } else {
                    Lookup::Backoff
                }
            }
            None => Lookup::Missing,
        }
    }

    pub fn mark_in_flight(&mut self, k: ShowKey) {
        self.entries.insert(k, Entry::InFlight);
        self.evict();
    }

    pub fn insert_hit(&mut self, k: ShowKey, detail: ModelDetail, now: Instant) {
        self.entries.insert(
            k,
            Entry::Hit {
                detail: Arc::new(detail),
                fetched_at: now,
            },
        );
        self.evict();
    }

    pub fn insert_miss(&mut self, k: ShowKey, now: Instant) {
        let failures = match self.entries.get(&k) {
            Some(Entry::Miss { failures, .. }) => failures.saturating_add(1),
            _ => 1,
        };
        let backoff = Self::BACKOFF_BASE
            .saturating_mul(1u32 << (failures - 1).min(31))
            .min(Self::BACKOFF_MAX);
        self.entries.insert(
            k,
            Entry::Miss {
                failures,
                retry_after: now + backoff,
            },
        );
        self.evict();
    }

    /// Drop entries for models that are no longer present, keeping the map
    /// bounded on a box that cycles through hundreds of models.
    pub fn retain_live(&mut self, live: &HashSet<ShowKey>) {
        if self.entries.len() <= self.capacity {
            return;
        }
        self.entries.retain(|k, _| live.contains(k));
        self.evict();
    }

    fn evict(&mut self) {
        while self.entries.len() > self.capacity {
            self.entries.shift_remove_index(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail() -> ModelDetail {
        ModelDetail {
            architecture: Some("examplemoe".into()),
            ..Default::default()
        }
    }

    #[test]
    fn digest_keyed_entries_never_expire() {
        let now = Instant::now();
        let k = ShowKey::new("example-moe:q8", Some("aaaa1111bbbb"));
        assert!(k.is_digest_keyed());

        let mut c = ShowCache::default();
        c.insert_hit(k.clone(), detail(), now);

        let much_later = now + Duration::from_secs(86_400);
        assert_eq!(c.lookup(&k, much_later), Lookup::Hit);
        assert!(c.get(&k, much_later).is_some());
    }

    #[test]
    fn name_keyed_entries_expire() {
        let now = Instant::now();
        let k = ShowKey::new("example-moe:q8", None);
        assert!(!k.is_digest_keyed());

        let mut c = ShowCache::default();
        c.insert_hit(k.clone(), detail(), now);
        assert_eq!(c.lookup(&k, now + Duration::from_secs(60)), Lookup::Hit);
        assert_eq!(c.lookup(&k, now + Duration::from_secs(601)), Lookup::Missing);
    }

    #[test]
    fn failures_back_off_exponentially_but_never_permanently() {
        let now = Instant::now();
        let k = ShowKey::new("gone:latest", Some("dead"));
        let mut c = ShowCache::default();

        c.insert_miss(k.clone(), now);
        assert_eq!(c.lookup(&k, now), Lookup::Backoff);
        assert_eq!(c.lookup(&k, now + Duration::from_secs(31)), Lookup::Missing);

        // Second failure -> 60s, third -> 120s.
        c.insert_miss(k.clone(), now);
        assert_eq!(c.lookup(&k, now + Duration::from_secs(31)), Lookup::Backoff);
        assert_eq!(c.lookup(&k, now + Duration::from_secs(61)), Lookup::Missing);

        // Capped at 10 minutes, and still eventually retried.
        for _ in 0..20 {
            c.insert_miss(k.clone(), now);
        }
        assert_eq!(c.lookup(&k, now + Duration::from_secs(601)), Lookup::Missing);
    }

    #[test]
    fn in_flight_prevents_a_second_fetch() {
        let now = Instant::now();
        let k = ShowKey::new("x", Some("y"));
        let mut c = ShowCache::default();
        assert_eq!(c.lookup(&k, now), Lookup::Missing);
        c.mark_in_flight(k.clone());
        assert_eq!(c.lookup(&k, now), Lookup::Loading);
    }

    #[test]
    fn cache_stays_bounded() {
        let now = Instant::now();
        let mut c = ShowCache::new(4);
        for i in 0..20 {
            c.insert_hit(ShowKey::new(&format!("m{i}"), Some(&format!("d{i}"))), detail(), now);
        }
        assert_eq!(c.entries.len(), 4);
        // The oldest were evicted, the newest survive.
        assert!(c.get(&ShowKey::new("m19", Some("d19")), now).is_some());
        assert!(c.get(&ShowKey::new("m0", Some("d0")), now).is_none());
    }
}
