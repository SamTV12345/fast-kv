use linked_hash_map::LinkedHashMap;
use serde_json::Value;

struct LRU<K, V>
where
    K : Eq + std::hash::Hash + Clone, V: Clone {
    capacity: usize,
    cache: LinkedHashMap<K, V>,
    evictable: fn(K,V) -> bool
}

#[derive(Clone)]
pub struct MapVal {
    pub value: Value,
    pub dirty: bool,
    pub writing_in_progress: bool
}


impl<K, V> LRU<K, V>
where
    K: std::hash::Hash + Eq + Clone, V: Clone
{
    fn new(capacity: usize, evictable: fn(K,V) -> bool) -> Self {
        LRU {
            capacity,
            cache: LinkedHashMap::new(),
            evictable,
        }
    }

    /**
    * @param isUse Optional boolean indicating whether this get() should be considered a "use" of the
    *     entry (for determining least recently used). Defaults to true.
    * @returns undefined if there is no entry matching the given key.
    */
    fn get(&mut self, key: K, is_use: Option<bool>) -> Option<V> {
        let is_use = is_use.unwrap_or(true);
        if !self.cache.contains_key(&key) {
            return None
        }
        let v = self.cache.get(&key).map(|v| v.clone());

        if is_use {
            // Mark this entry as the most recently used entry.
            let value = self.cache.remove(&key).unwrap();
            self.cache.insert(key, value);
        }
        v
    }

    fn put(&mut self, key: K, value: V) {
        if self.cache.len() == self.capacity {
            let key = self.cache.keys().next().unwrap().clone();
            if (self.evictable)(key.clone(), self.cache.remove(&key).unwrap()) {
                self.cache.insert(key, value);
            }
        } else {
            self.cache.insert(key, value);
        }
    }

    /**
    * Adds or updates an entry in the cache. This marks the entry as the most recently used entry.
    */
    fn set(&mut self, key: K, value: V) {
        self.cache.remove(&key);
        self.cache.insert(key, value);

    }

    /**
    * Evicts the oldest evictable entries until the number of entries is equal to or less than the
    * cache's capacity. This method is automatically called by set(). Call this if you need to evict
    * newly evictable entries before the next call to set().
    */
    fn evict_old(&mut self) {
        for (k, v) in self.cache.clone().iter() {
            if self.cache.len() <= self.capacity {
                break
            }
            if !(self.evictable)(k.clone(), v.clone()) {
                continue
            }
            self.cache.remove(&k);
        }
    }
}


pub struct DBSettings {
    pub bulk_limit: usize,
    pub cache: usize,
    write_interval: u64,
    json: bool,
    charset: String
}

impl Default for DBSettings {
    fn default() -> Self {
        DBSettings {
            bulk_limit: 0,
            cache: 10000,
            write_interval: 100,
            json: true,
            charset: "utf8mb4".to_string()
        }
    }
}


pub struct Database {
    buffer: LRU<String, MapVal>,
    settings: DBSettings
}

impl Database {
    pub fn new() -> Self {
        Database {
            buffer: LRU::new(10000, |k,v:MapVal|!v.dirty && !v.writing_in_progress),
            settings: DBSettings::default()

        }
    }
}