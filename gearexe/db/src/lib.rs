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

//! Database library for gearexe.

use gear_core::utils;
use gprimitives::H256;

mod database;
pub mod iterator;
mod mem;
mod overlay;
mod rocks;
pub mod verifier;
pub mod visitor;

pub use database::Database;
pub use mem::MemDb;
pub use rocks::RocksDatabase;

pub fn hash(data: &[u8]) -> H256 {
    utils::hash(data).into()
}

/// Content-addressable storage database.
pub trait CASDatabase: Send + Sync {
    /// Clone ref to database instance.
    fn clone_boxed(&self) -> Box<dyn CASDatabase>;

    /// Read data by hash.
    fn read(&self, hash: H256) -> Option<Vec<u8>>;

    /// Check if data exists by key.
    fn contains(&self, hash: H256) -> bool;

    /// Write data, returns data hash.
    fn write(&self, data: &[u8]) -> H256;
}

/// Key-value database.
pub trait KVDatabase: Send + Sync {
    /// Clone ref to key-value database instance.
    fn clone_boxed(&self) -> Box<dyn KVDatabase>;

    /// Get value by key.
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;

    /// Take (get and remove) value by key.
    fn take(&self, key: &[u8]) -> Option<Vec<u8>>;

    /// Check if data exists by key.
    fn contains(&self, key: &[u8]) -> bool;

    /// Put (insert) value by key.
    fn put(&self, key: &[u8], data: Vec<u8>);

    fn iter_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a>;
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, thread};

    use crate::{CASDatabase, KVDatabase};

    fn to_big_vec(x: u32) -> Vec<u8> {
        let bytes = x.to_le_bytes();
        bytes
            .iter()
            .cycle()
            .take(1024 * 1024)
            .copied()
            .collect::<Vec<_>>()
    }

    pub fn is_clonable<DB: Clone>(db: DB) {
        let _ = db.clone();
    }

    pub fn cas_read_write<DB: CASDatabase>(db: DB) {
        let data = b"Hello, world!";
        let hash = db.write(data);
        assert_eq!(db.read(hash), Some(data.to_vec()));
    }

    pub fn kv_read_write<DB: KVDatabase>(db: DB) {
        let key = b"key";
        let data = b"value".to_vec();
        db.put(key, data.clone());
        assert_eq!(db.get(key.as_slice()), Some(data));
    }

    pub fn kv_iter_prefix<DB: KVDatabase>(db: DB) {
        let testcase = |prefix: &str, expectations: &[(&str, &str)]| {
            let actual: BTreeSet<_> = db.iter_prefix(prefix.as_bytes()).collect();
            let expected: BTreeSet<_> = expectations
                .iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
                .collect();
            assert_eq!(actual, expected);
        };

        db.put(b"prefix_foo", b"hello".to_vec());
        db.put(b"prefix_bar", b"world".to_vec());

        testcase(
            "prefix_",
            &[("prefix_foo", "hello"), ("prefix_bar", "world")],
        );

        testcase("", &[("prefix_foo", "hello"), ("prefix_bar", "world")]);

        testcase("0", &[]);

        testcase("prefix_foobar", &[]);

        testcase("prefix_foo", &[("prefix_foo", "hello")]);

        testcase("prefix_bar", &[("prefix_bar", "world")]);
    }

    pub fn cas_multi_thread<DB: CASDatabase>(db: DB) {
        let amount = 10;

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
            let expected = to_big_vec(x);
            let data = db.read(crate::hash(expected.as_slice()));
            assert_eq!(data, Some(expected));
        }
    }

    pub fn kv_multi_thread<DB: KVDatabase>(db: DB) {
        let amount = 10;

        let db_clone = KVDatabase::clone_boxed(&db);
        let handler1 = thread::spawn(move || {
            for x in 0u32..amount {
                db_clone.put(x.to_le_bytes().as_slice(), to_big_vec(x));
            }
        });

        let db_clone = KVDatabase::clone_boxed(&db);
        let handler2 = thread::spawn(move || {
            for x in amount..amount * 2 {
                db_clone.put(x.to_le_bytes().as_slice(), to_big_vec(x));
            }
        });

        handler1.join().unwrap();
        handler2.join().unwrap();

        for x in 0u32..amount * 2 {
            let expected = to_big_vec(x);
            let data = db.get(x.to_le_bytes().as_slice());
            assert_eq!(data, Some(expected));
        }
    }
}
