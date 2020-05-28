use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Local};
use tokio::net::UdpSocket;

use crate::server::ConnId;

pub struct Cache {
    timeout: Duration,
    by_id: HashMap<ConnId, CacheEntryOuter>,
    expired: RefCell<HashSet<ConnId>>,
}

struct CacheEntryOuter {
    last_access: Cell<DateTime<Local>>,
    data: CacheEntry,
}

pub struct CacheEntry {
    pub id: ConnId,
    pub socket: UdpSocket,
}

impl Cache {
    pub fn new(timeout: Duration) -> Self {
        Cache {
            timeout,
            by_id: Default::default(),
            expired: Default::default(),
        }
    }

    pub fn insert(&mut self, id: ConnId, socket: UdpSocket) -> &mut CacheEntry {
        self.cleanup();
        let now = Local::now();
        let data = CacheEntry { id, socket };
        let entry = CacheEntryOuter { last_access: Cell::new(now), data };
        self.by_id.insert(id, entry);
        &mut self.by_id.get_mut(&id).unwrap().data
    }

    pub fn get_by_id_mut(&mut self, id: ConnId) -> Option<&mut CacheEntry> {
        Cache::prepare_entry_mut(self.by_id.get_mut(&id)?, self.timeout, &self.expired)
    }

    fn prepare_entry<'a>(&self, e: &'a CacheEntryOuter) -> Option<&'a CacheEntry> {
        let now = Local::now();
        if now.signed_duration_since(e.last_access.get()) > self.timeout {
            self.expired.borrow_mut().insert(e.data.id);
            return None;
        }
        e.last_access.set(now);
        Some(&e.data)
    }

    fn prepare_entry_mut<'a>(e: &'a mut CacheEntryOuter, timeout: Duration, expired: &RefCell<HashSet<ConnId>>) -> Option<&'a mut CacheEntry> {
        let now = Local::now();
        if now.signed_duration_since(e.last_access.get()) > timeout {
            expired.borrow_mut().insert(e.data.id);
            return None;
        }
        e.last_access.set(now);
        Some(&mut e.data)
    }

    pub fn iter(&self) -> impl Iterator<Item=&CacheEntry> {
        self.by_id.values().filter_map(move |v| self.prepare_entry(v))
    }

    pub fn len_max(&self) -> usize {
        self.by_id.len()
    }

    pub fn cleanup(&mut self) {
        let vec = self.expired.get_mut();
        for x in vec.drain() {
            self.by_id.remove(&x);
        }
    }
}