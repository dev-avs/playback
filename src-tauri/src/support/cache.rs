//! A small in-memory cache that keeps the newest entries.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// Holds at most `cap` entries; the oldest is dropped first.
pub struct Cache<V> {
    cap: usize,
    inner: Mutex<Entries<V>>,
}

struct Entries<V> {
    map: HashMap<String, V>,
    order: VecDeque<String>,
}

impl<V: Clone> Cache<V> {
    pub fn new(cap: usize) -> Self {
        Cache { cap, inner: Mutex::new(Entries { map: HashMap::new(), order: VecDeque::new() }) }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        self.inner.lock().unwrap().map.get(key).cloned()
    }

    pub fn put(&self, key: String, value: V) {
        let mut e = self.inner.lock().unwrap();
        if e.map.insert(key.clone(), value).is_none() {
            e.order.push_back(key);
        }
        while e.order.len() > self.cap {
            if let Some(old) = e.order.pop_front() {
                e.map.remove(&old);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;

    #[test]
    fn drops_the_oldest_past_its_cap() {
        let c = Cache::new(2);
        c.put("a".into(), 1);
        c.put("b".into(), 2);
        c.put("a".into(), 3); // an update, not a new entry
        c.put("c".into(), 4);
        assert_eq!(c.get("a"), None);
        assert_eq!(c.get("b"), Some(2));
        assert_eq!(c.get("c"), Some(4));
    }
}
