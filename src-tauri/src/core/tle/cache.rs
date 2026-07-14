//! In-memory cache for `TleRecord`s keyed by NORAD ID.
//!
//! Purpose: avoid hitting the DB on every tracking tick (500 ms) and every
//! pass-planner sample (thousands per request). The cache loads lazily —
//! the first lookup for a NORAD pulls the row from the DB and stores it;
//! subsequent lookups are pure memory reads.
//!
//! Discipline (see `knowledge/db.md` — "TLE cache"):
//! - Anything that mutates `satellites_tle` (upsert, delete) MUST call
//!   `TleCache::invalidate(norad)` afterwards. The repo layer does not call
//!   into the cache; the caller wires both.
//! - The cache holds no DB handle of its own — `get_or_load` takes a `&Database`.
//! - All operations are thread-safe (`RwLock`); read-heavy workload favors
//!   `RwLock` over `Mutex`.

use std::collections::HashMap;
use std::sync::RwLock;

use super::repo;
use super::{TleError, TleRecord};
use crate::core::db::Database;

#[derive(Debug, Default)]
struct CacheState {
    generation: u64,
    records: HashMap<u32, TleRecord>,
}

#[derive(Debug, Default)]
pub struct TleCache {
    inner: RwLock<CacheState>,
}

impl TleCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the cached record for `norad`, loading from DB on miss.
    /// Returns `Ok(None)` when the satellite has no TLE in the DB.
    pub fn get_or_load(&self, db: &Database, norad: u32) -> Result<Option<TleRecord>, TleError> {
        let observed_generation = match self.inner.read() {
            Ok(state) => {
                if let Some(record) = state.records.get(&norad) {
                    return Ok(Some(record.clone()));
                }
                Some(state.generation)
            }
            Err(_) => None,
        };

        let loaded = repo::get_by_norad(db, norad)?;
        if let (Some(record), Some(generation)) = (&loaded, observed_generation) {
            self.insert_if_current(norad, record, generation);
        }
        Ok(loaded)
    }

    /// Remove a single entry. Call after upsert/delete of that NORAD.
    pub fn invalidate(&self, norad: u32) {
        if let Ok(mut state) = self.inner.write() {
            state.generation = state.generation.wrapping_add(1);
            state.records.remove(&norad);
        }
    }

    /// Drop all entries. Use after bulk upserts (e.g., catalog sync).
    pub fn invalidate_all(&self) {
        if let Ok(mut state) = self.inner.write() {
            state.generation = state.generation.wrapping_add(1);
            state.records.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .map(|state| state.records.len())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn insert_if_current(&self, norad: u32, record: &TleRecord, observed_generation: u64) -> bool {
        let Ok(mut state) = self.inner.write() else {
            return false;
        };
        if state.generation != observed_generation {
            return false;
        }
        state.records.insert(norad, record.clone());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tle::parser::parse_tle;

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    fn seeded_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        repo::upsert(&db, &rec, "test").unwrap();
        db
    }

    #[test]
    fn miss_then_load_then_hit() {
        let db = seeded_db();
        let cache = TleCache::new();
        assert!(cache.is_empty());

        let first = cache.get_or_load(&db, 25544).unwrap().unwrap();
        assert_eq!(first.norad_id, 25544);
        assert_eq!(cache.len(), 1);

        let second = cache.get_or_load(&db, 25544).unwrap().unwrap();
        assert_eq!(second.epoch, first.epoch);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn missing_norad_returns_none_and_does_not_cache() {
        let db = seeded_db();
        let cache = TleCache::new();
        assert!(cache.get_or_load(&db, 99999).unwrap().is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn invalidate_forces_reload() {
        let db = seeded_db();
        let cache = TleCache::new();
        cache.get_or_load(&db, 25544).unwrap();
        assert_eq!(cache.len(), 1);

        cache.invalidate(25544);
        assert_eq!(cache.len(), 0);

        cache.get_or_load(&db, 25544).unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalidate_all_clears_every_entry() {
        let db = seeded_db();
        let cache = TleCache::new();
        cache.get_or_load(&db, 25544).unwrap();
        assert_eq!(cache.len(), 1);

        cache.invalidate_all();
        assert!(cache.is_empty());
    }

    #[test]
    fn stale_load_cannot_repopulate_cache_after_invalidation() {
        use chrono::Duration;

        let db = seeded_db();
        let cache = TleCache::new();
        let observed_generation = cache.inner.read().unwrap().generation;
        let stale = repo::get_by_norad(&db, 25544).unwrap().unwrap();

        let mut newer = stale.clone();
        newer.name = "ISS (UPDATED)".to_string();
        newer.epoch += Duration::hours(1);
        repo::upsert(&db, &newer, "test").unwrap();
        cache.invalidate_all();

        assert!(!cache.insert_if_current(25544, &stale, observed_generation));
        assert!(cache.is_empty());

        let loaded = cache.get_or_load(&db, 25544).unwrap().unwrap();
        assert_eq!(loaded.name, "ISS (UPDATED)");
        assert_eq!(loaded.epoch, newer.epoch);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalidating_a_missing_entry_still_advances_generation() {
        let cache = TleCache::new();
        let before = cache.inner.read().unwrap().generation;

        cache.invalidate(99999);

        let after = cache.inner.read().unwrap().generation;
        assert_eq!(after, before.wrapping_add(1));
    }

    #[test]
    fn concurrent_readers_do_not_block() {
        use std::sync::Arc;
        use std::thread;
        let db = seeded_db();
        let cache = Arc::new(TleCache::new());
        cache.get_or_load(&db, 25544).unwrap();

        let mut handles = vec![];
        for _ in 0..8 {
            let c = Arc::clone(&cache);
            let d = db.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let r = c.get_or_load(&d, 25544).unwrap().unwrap();
                    assert_eq!(r.norad_id, 25544);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(cache.len(), 1);
    }
}
