// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{CASDatabase, KVDatabase, MemDb};
use gear_core::utils;
use gprimitives::H256;
use std::collections::HashSet;

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
}

impl KVOverlay {
    pub fn new(db: Box<dyn KVDatabase>) -> Self {
        Self {
            db,
            mem: MemDb::default(),
        }
    }
}

impl KVDatabase for KVOverlay {
    fn clone_boxed(&self) -> Box<dyn KVDatabase> {
        Box::new(Self {
            db: self.db.clone_boxed(),
            mem: self.mem.clone(),
        })
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.mem.get(key).or_else(|| self.db.get(key))
    }

    unsafe fn take(&self, _key: &[u8]) -> Option<Vec<u8>> {
        unimplemented!()
    }

    fn contains(&self, key: &[u8]) -> bool {
        KVDatabase::contains(&self.mem, key) || KVDatabase::contains(&*self.db, key)
    }

    fn put(&self, key: &[u8], value: Vec<u8>) {
        self.mem.put(key, value)
    }

    fn iter_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a> {
        let mem_iter = self.mem.iter_prefix(prefix);
        let db_iter = self.db.iter_prefix(prefix);

        let full_iter = mem_iter.chain(db_iter);

        let mut known_keys = HashSet::new();

        let filtered_iter = full_iter
            .filter_map(move |(k, v)| known_keys.insert(utils::hash(&k)).then_some((k, v)));

        Box::new(filtered_iter)
    }

    fn is_empty(&self) -> bool {
        unimplemented!()
    }
}
