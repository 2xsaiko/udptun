use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::rc::Rc;

use chrono::{DateTime, Duration, Local};
use num_traits::cast::ToPrimitive;
use thiserror::Error;

pub struct Cache {
    timeout: Duration,
    ids: Vec<u8>,
    by_id: HashMap<u8, Rc<CacheEntry>>,
    by_addr: HashMap<SocketAddr, Rc<CacheEntry>>,
    expired: RefCell<HashSet<SocketId>>,
}

struct CacheEntry {
    last_access: Cell<DateTime<Local>>,
    data: SocketId,
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub struct SocketId {
    pub id: u8,
    pub addr: SocketAddr,
}

impl Cache {
    pub fn new(timeout: Duration) -> Self {
        Cache {
            timeout,
            ids: Vec::new(),
            by_id: Default::default(),
            by_addr: Default::default(),
            expired: Default::default(),
        }
    }

    pub fn insert(&mut self, id: Option<u8>, addr: SocketAddr) -> Result<SocketId, Error> {
        self.cleanup();
        let now = Local::now();
        let id = id.or_else(|| self.get_next_free_id()).ok_or(Error::NoFreeSlots)?;
        if let Err(pos) = self.ids.binary_search(&id) {
            self.ids.insert(pos, id)
        }
        let data = SocketId { id, addr };
        let entry = Rc::new(CacheEntry { last_access: Cell::new(now), data });
        self.by_addr.insert(data.addr, entry.clone());
        self.by_id.insert(data.id, entry);
        Ok(data)
    }

    pub fn get_or_insert_by_addr(&mut self, addr: SocketAddr) -> Result<SocketId, Error> {
        match self.get_by_addr(addr) {
            None => self.insert(None, addr),
            Some(r) => Ok(r),
        }
    }

    pub fn get_by_id(&self, id: u8) -> Option<SocketId> {
        self.prepare_entry(self.by_id.get(&id)?)
    }

    pub fn get_by_addr(&self, addr: SocketAddr) -> Option<SocketId> {
        self.prepare_entry(self.by_addr.get(&addr)?)
    }

    fn prepare_entry(&self, e: &Rc<CacheEntry>) -> Option<SocketId> {
        let now = Local::now();
        if now.signed_duration_since(e.last_access.get()) > self.timeout {
            self.expired.borrow_mut().insert(e.data);
            return None;
        }
        e.last_access.set(now);
        Some(e.data)
    }

    fn get_next_free_id(&self) -> Option<u8> {
        self.ids.iter()
            .enumerate()
            .find_map(|(exp, &v)| if v != exp as u8 { Some(exp as u8) } else { None })
            .or_else(|| self.ids.len().to_u8())
    }

    pub fn cleanup(&mut self) {
        let vec = self.expired.get_mut();
        for x in vec.drain() {
            if let Ok(pos) = self.ids.binary_search(&x.id) {
                self.ids.remove(pos);
            }
            self.by_id.remove(&x.id);
            self.by_addr.remove(&x.addr);
        }
    }
}

#[derive(Error, Debug, Copy, Clone)]
pub enum Error {
    #[error("no free ID slots available")]
    NoFreeSlots
}