//! PNGs from requested Office pages, scoped to one retained layout UUID.
//! No source paths, disk files, prefetch or reuse across separate opens.
use std::sync::Arc;
use uuid::Uuid;

const VIEW_PAGES: usize = 6;
const VIEW_BYTES: usize = 24 * 1024 * 1024;
const TOTAL_PAGES: usize = 48;
const TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct Page {
    pub count: usize,
    pub bytes: Vec<u8>,
}

struct Entry {
    view: Uuid,
    index: usize,
    page: Arc<Page>,
}

pub(super) struct Cache {
    // Oldest first; at most 48 entries, so no secondary index is needed.
    entries: Vec<Entry>,
    bytes: usize,
    limits: Limits,
}

#[derive(Clone, Copy)]
struct Limits {
    view_pages: usize,
    view_bytes: usize,
    total_pages: usize,
    total_bytes: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new(Limits {
            view_pages: VIEW_PAGES,
            view_bytes: VIEW_BYTES,
            total_pages: TOTAL_PAGES,
            total_bytes: TOTAL_BYTES,
        })
    }
}

impl Cache {
    fn new(limits: Limits) -> Self {
        Self {
            entries: Vec::new(),
            bytes: 0,
            limits,
        }
    }

    fn position(&self, view: Uuid, index: usize) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.view == view && entry.index == index)
    }

    pub(super) fn get(&mut self, view: Uuid, index: usize) -> Option<Arc<Page>> {
        let position = self.position(view, index)?;
        let entry = self.entries.remove(position);
        let page = Arc::clone(&entry.page);
        self.entries.push(entry);
        Some(page)
    }

    fn evict(&mut self, position: usize) {
        self.bytes -= self.entries.remove(position).page.bytes.len();
    }

    pub(super) fn insert(&mut self, view: Uuid, index: usize, page: Arc<Page>) -> bool {
        let size = page.bytes.len();
        // Oversized pages can still be displayed; retaining one must never
        // push out all other views or exceed either byte budget.
        if size == 0
            || size > self.limits.view_bytes
            || size > self.limits.total_bytes
            || self.limits.view_pages == 0
            || self.limits.total_pages == 0
            || index >= page.count
        {
            return false;
        }
        if let Some(position) = self.position(view, index) {
            self.evict(position);
        }
        let (mut count, mut view_bytes) = self
            .entries
            .iter()
            .filter(|entry| entry.view == view)
            .fold((0, 0), |(count, bytes), entry| {
                (count + 1, bytes + entry.page.bytes.len())
            });
        while count >= self.limits.view_pages || view_bytes + size > self.limits.view_bytes {
            let position = self
                .entries
                .iter()
                .position(|entry| entry.view == view)
                .unwrap();
            view_bytes -= self.entries[position].page.bytes.len();
            count -= 1;
            self.evict(position);
        }
        while self.entries.len() >= self.limits.total_pages
            || self.bytes + size > self.limits.total_bytes
        {
            self.evict(0);
        }
        self.bytes += size;
        self.entries.push(Entry { view, index, page });
        true
    }

    pub(super) fn remove_view(&mut self, view: Uuid) {
        self.entries.retain(|entry| entry.view != view);
        self.bytes = self
            .entries
            .iter()
            .map(|entry| entry.page.bytes.len())
            .sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(size: usize) -> Arc<Page> {
        Arc::new(Page {
            count: 10,
            bytes: vec![7; size],
        })
    }
    fn limits(
        view_pages: usize,
        view_bytes: usize,
        total_pages: usize,
        total_bytes: usize,
    ) -> Cache {
        Cache::new(Limits {
            view_pages,
            view_bytes,
            total_pages,
            total_bytes,
        })
    }

    #[test]
    fn recent_pages_obey_each_views_page_and_byte_limits() {
        let view = Uuid::new_v4();
        let mut cache = limits(2, 10, 10, 100);
        assert!(cache.insert(view, 0, page(4)));
        assert!(cache.insert(view, 1, page(4)));
        assert!(cache.get(view, 0).is_some());
        assert!(cache.insert(view, 2, page(4)));
        assert!(cache.get(view, 1).is_none());
        assert!(cache.get(view, 0).is_some());
        assert!(cache.insert(view, 3, page(7)));
        assert!(cache.get(view, 0).is_none());
        assert!(cache.get(view, 2).is_none());
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.bytes, 7);
    }

    #[test]
    fn multiple_open_views_share_global_page_and_byte_budgets() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut cache = limits(6, 30, 3, 12);
        for index in 0..3 {
            assert!(cache.insert(first, index, page(4)));
        }
        assert!(cache.get(first, 0).is_some());
        assert!(cache.insert(second, 0, page(4)));
        assert!(cache.get(first, 1).is_none());
        assert_eq!(cache.entries.len(), 3);
        assert_eq!(cache.bytes, 12);
        assert!(cache.insert(second, 1, page(9)));
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.bytes, 9);
        assert!(cache.get(second, 1).is_some());
    }

    #[test]
    fn close_drops_pages_and_separate_opens_never_share_cached_bytes() {
        let first = Uuid::new_v4();
        let reopened = Uuid::new_v4();
        let mut cache = Cache::default();
        let image = page(4);
        let weak = Arc::downgrade(&image);
        cache.insert(first, 0, image);
        assert!(cache.get(reopened, 0).is_none());
        cache.remove_view(first);
        cache.remove_view(first);
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.bytes, 0);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn invalid_or_oversized_pages_are_not_retained_and_lru_order_is_preserved() {
        let view = Uuid::new_v4();
        let mut cache = limits(2, 10, 2, 10);
        assert!(!cache.insert(view, 0, page(11)));
        assert!(!cache.insert(view, 10, page(1)));
        assert!(!cache.insert(view, 0, page(0)));
        cache.insert(view, 0, page(4));
        cache.insert(view, 1, page(4));
        assert!(cache.get(view, 0).is_some());
        cache.insert(view, 2, page(4));
        assert!(cache.get(view, 0).is_some());
        assert!(cache.get(view, 1).is_none());
        assert_eq!(cache.bytes, 8);
    }
}
