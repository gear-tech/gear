// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{CASDatabase, KVDatabase, MemDb};
use dashmap::DashSet;
use gear_core::utils;
use gprimitives::H256;
use std::{collections::HashSet, sync::Arc};

pub struct CASOverlay {
    db: Box<dyn CASDatabase>,
    mem: MemDb,
}

impl CASOverlay {
    pub fn new(db: Box<dyn CASDatabase>) -> Self {
        Self {
            db,
            mem: MemDb::default(),
        }
    }
}

impl CASDatabase for CASOverlay {
    fn clone_boxed(&self) -> Box<dyn CASDatabase> {
        Box::new(Self {
            db: self.db.clone_boxed(),
            mem: self.mem.clone(),
        })
    }

    fn read(&self, hash: H256) -> Option<Vec<u8>> {
        self.mem.read(hash).or_else(|| self.db.read(hash))
    }

    fn contains(&self, hash: H256) -> bool {
        CASDatabase::contains(&self.mem, hash) || CASDatabase::contains(&*self.db, hash)
    }

    fn write(&self, data: &[u8]) -> H256 {
        self.mem.write(data)
    }
}

pub struct KVOverlay {
    db: Box<dyn KVDatabase>,
    mem: MemDb,
    erased_keys: Arc<DashSet<Vec<u8>>>,
}

impl KVOverlay {
    pub fn new(db: Box<dyn KVDatabase>) -> Self {
        Self {
            db,
            mem: MemDb::default(),
            erased_keys: Default::default(),
        }
    }

    fn is_erased(&self, key: &[u8]) -> bool {
        self.erased_keys.contains(key)
    }

    fn erase(&self, key: Vec<u8>) -> bool {
        self.erased_keys.insert(key)
    }
}

impl KVDatabase for KVOverlay {
    fn clone_boxed(&self) -> Box<dyn KVDatabase> {
        Box::new(Self {
            db: self.db.clone_boxed(),
            mem: self.mem.clone(),
            erased_keys: self.erased_keys.clone(),
        })
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.mem.get(key).or_else(|| {
            if !self.is_erased(key) {
                self.db.get(key)
            } else {
                None
            }
        })
    }

    unsafe fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        if !self.is_erased(key) {
            unsafe {
                self.mem.take(key).or_else(|| {
                    self.db.get(key).inspect(|_| {
                        self.erase(key.to_vec());
                    })
                })
            }
        } else {
            None
        }
    }

    fn contains(&self, key: &[u8]) -> bool {
        KVDatabase::contains(&self.mem, key)
            || (!self.is_erased(key) && KVDatabase::contains(&*self.db, key))
    }

    fn put(&self, key: &[u8], value: Vec<u8>) {
        self.erased_keys.remove(key);
        self.mem.put(key, value)
    }

    fn iter_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a> {
        let mem_iter = self.mem.iter_prefix(prefix);
        let db_iter = self
            .db
            .iter_prefix(prefix)
            .filter(|(key, _)| !self.is_erased(key));

        let full_iter = mem_iter.chain(db_iter);

        let mut known_keys = HashSet::new();

        let filtered_iter = full_iter
            .filter_map(move |(k, v)| known_keys.insert(utils::hash(&k)).then_some((k, v)));

        Box::new(filtered_iter)
    }

    fn is_empty(&self) -> bool {
        self.mem.is_empty()
            && match (self.db.is_empty(), self.erased_keys.is_empty()) {
                (true, _) => true,
                (false, true) => false,
                (false, false) => {
                    unimplemented!()
                }
            }
    }
}
