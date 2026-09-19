use std::collections::HashMap;

#[derive(Debug)]
struct CacheEntry {
    bytes: Vec<u8>,
    last_used: u64,
}

#[derive(Debug)]
pub struct PageCache {
    page_size: usize,
    max_pages: usize,
    access_counter: u64,
    entries: HashMap<u64, CacheEntry>,
}

impl PageCache {
    pub fn new(page_size: usize, max_pages: usize) -> Self {
        Self {
            page_size,
            max_pages,
            access_counter: 0,
            entries: HashMap::new(),
        }
    }

    pub fn get(&mut self, offset: u64) -> Option<&[u8]> {
        self.access_counter = self.access_counter.wrapping_add(1);
        let entry = self.entries.get_mut(&offset)?;
        entry.last_used = self.access_counter;
        Some(entry.bytes.as_slice())
    }

    pub fn insert(&mut self, offset: u64, bytes: Vec<u8>) {
        if self.max_pages == 0 {
            return;
        }

        self.access_counter = self.access_counter.wrapping_add(1);
        self.entries.insert(
            offset,
            CacheEntry {
                bytes,
                last_used: self.access_counter,
            },
        );

        while self.entries.len() > self.max_pages {
            let least_recent = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(&offset, _)| offset);
            if let Some(offset) = least_recent {
                self.entries.remove(&offset);
            } else {
                break;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn cached_bytes(&self) -> usize {
        self.entries.values().map(|entry| entry.bytes.len()).sum()
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn max_pages(&self) -> usize {
        self.max_pages
    }

    pub fn is_usable(&self) -> bool {
        self.page_size != 0 && self.max_pages != 0
    }
}

#[cfg(test)]
mod tests {
    use super::PageCache;

    #[test]
    fn cache_evicts_least_recent_page() {
        let mut cache = PageCache::new(4, 2);
        assert!(cache.is_empty());
        cache.insert(0, vec![0; 4]);
        assert!(!cache.is_empty());
        cache.insert(4, vec![1; 4]);
        assert!(cache.get(0).is_some());
        cache.insert(8, vec![2; 4]);
        assert!(cache.get(4).is_none());
    }
}
