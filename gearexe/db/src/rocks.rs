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

use crate::{CASDatabase, KVDatabase};
use anyhow::Result;
use gprimitives::H256;
use rocksdb::{DB, DBIteratorWithThreadMode, Options};
use std::{path::PathBuf, sync::Arc};

/// Database for storing states and codes in memory.
#[derive(Debug, Clone)]
pub struct RocksDatabase {
    inner: Arc<DB>,
}

impl RocksDatabase {
    /// Open database at specified
    pub fn open(path: PathBuf) -> Result<Self> {
        let db = DB::open(&configure_rocksdb(), path)?;
        Ok(Self {
            inner: Arc::new(db),
        })
    }
}

impl CASDatabase for RocksDatabase {
    fn clone_boxed(&self) -> Box<dyn CASDatabase> {
        Box::new(self.clone())
    }

    fn read(&self, hash: H256) -> Option<Vec<u8>> {
        self.inner
            .get(hash.as_bytes())
            .expect("Failed to read data, database is not in valid state")
    }

    fn contains(&self, hash: H256) -> bool {
        self.inner.key_may_exist(hash)
            && self
                .inner
                .get_pinned(hash)
                .expect("Failed to read data, database is not in valid state")
                .is_some()
    }

    fn write(&self, data: &[u8]) -> H256 {
        let hash = crate::hash(data);
        self.inner
            .put(hash.as_bytes(), data)
            .expect("Failed to write data, database is not in valid state");
        hash
    }
}

impl KVDatabase for RocksDatabase {
    fn clone_boxed(&self) -> Box<dyn KVDatabase> {
        Box::new(self.clone())
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner
            .get(key)
            .expect("Failed to read data, database is not in valid state")
    }

    fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        let data = self
            .inner
            .get(key)
            .expect("Failed to read data, database is not in valid state");
        if data.is_some() {
            self.inner.delete(key).expect("Failed to delete data");
        }
        data
    }

    fn contains(&self, key: &[u8]) -> bool {
        self.inner.key_may_exist(key)
            && self
                .inner
                .get_pinned(key)
                .expect("Failed to read data, database is not in valid state")
                .is_some()
    }

    fn put(&self, key: &[u8], value: Vec<u8>) {
        self.inner
            .put(key, value)
            .expect("Failed to write data, database is not in valid state");
    }

    fn iter_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a> {
        Box::new(PrefixIterator {
            prefix,
            prefix_iter: self.inner.prefix_iterator(prefix),
            done: false,
        })
    }
}

pub struct PrefixIterator<'a> {
    prefix: &'a [u8],
    prefix_iter: DBIteratorWithThreadMode<'a, DB>,
    done: bool,
}

impl Iterator for PrefixIterator<'_> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        match self.prefix_iter.next() {
            Some(Ok((k, v))) if k.starts_with(self.prefix) => Some((k.to_vec(), v.to_vec())),
            Some(Err(e)) => panic!("Failed to read data, database is not in valid state: {e:?}"),
            _ => {
                self.done = true;
                None
            }
        }
    }
}

// TODO: Tune RocksDB configuration.
fn configure_rocksdb() -> Options {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.increase_parallelism(8);
    opts.set_max_background_jobs(4);
    opts.set_write_buffer_size(64 * 1024 * 1024);
    opts.set_max_write_buffer_number(3);
    opts.set_use_fsync(false);

    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;
    use tempfile::TempDir;

    fn with_database<F>(f: F)
    where
        F: FnOnce(RocksDatabase),
    {
        let temp_dir = scopeguard::guard(
            TempDir::new().expect("Failed to create a temporary directory"),
            |temp_dir| {
                temp_dir
                    .close()
                    .expect("Failed to close the temporary directory");
            },
        );

        let db =
            RocksDatabase::open(temp_dir.path().to_path_buf()).expect("Failed to open database");

        f(db);
    }

    #[test]
    fn is_clonable() {
        with_database(|db| {
            tests::is_clonable(db);
        });
    }

    #[test]
    fn cas_read_write() {
        with_database(|db| {
            tests::cas_read_write(db);
        });
    }

    #[test]
    fn kv_read_write() {
        with_database(|db| {
            tests::kv_read_write(db);
        });
    }

    #[test]
    fn kv_iter_prefix() {
        with_database(|db| {
            tests::kv_iter_prefix(db);
        });
    }

    #[test]
    fn cas_multi_thread() {
        with_database(|db| {
            tests::cas_multi_thread(db);
        });
    }

    #[test]
    fn kv_multi_thread() {
        with_database(|db| {
            tests::kv_multi_thread(db);
        });
    }
}
