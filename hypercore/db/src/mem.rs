// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::CASDatabase;
use dashmap::DashMap;
use gprimitives::H256;
use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct MemDb {
    inner: Arc<DashMap<H256, Vec<u8>>>,
}

impl CASDatabase for MemDb {
    fn clone_boxed(&self) -> Box<dyn CASDatabase> {
        Box::new(self.clone())
    }

    fn read(&self, hash: &H256) -> Option<Vec<u8>> {
        self.inner.get(hash).map(|v| v.value().clone())
    }

    fn write_by_hash(&self, hash: &H256, data: &[u8]) {
        debug_assert_eq!(*hash, crate::hash(data));
        self.inner.insert(*hash, data.to_vec());
    }
}

// TODO: Join tests for MemDb and RocksDb, making general tests for dyn CASDatabase.
#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn is_clonable() {
        let db = MemDb::default();
        let _ = db.clone();
    }

    #[test]
    fn read_write() {
        let db = MemDb::default();
        let data = b"Hello, world!";
        let hash = db.write(data);

        assert_eq!(db.read(&hash), Some(data.to_vec()));
    }

    #[test]
    fn multi_thread() {
        let amount = 10;

        let to_big_vec = |x: u32| -> Vec<u8> {
            let bytes = x.to_le_bytes();
            bytes
                .iter()
                .cycle()
                .take(1024 * 1024)
                .copied()
                .collect::<Vec<_>>()
        };

        let db = MemDb::default();

        let db_clone = CASDatabase::clone_boxed(&db);
        let handler1 = thread::spawn(move || {
            for x in 0u32..amount {
                db_clone.write(to_big_vec(x).as_slice());
            }
        });

        let db_clone = CASDatabase::clone_boxed(&db);
        let handler2 = thread::spawn(move || {
            for x in amount..amount * 2 {
                db_clone.write(to_big_vec(x).as_slice());
            }
        });

        handler1.join().unwrap();
        handler2.join().unwrap();

        for x in 0u32..amount * 2 {
            let data = to_big_vec(x);
            let hash = db.read(&crate::hash(data.as_slice()));
            assert_eq!(hash, Some(data));
        }
    }
}
